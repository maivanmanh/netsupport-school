import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'support/fake_store.dart';

class LiveAgent extends Agent with FakeStoreAgent {
  LiveAgent([FakeStore? shared]) : store = shared ?? FakeStore(), super([]);
  @override
  final FakeStore store;
  @override
  bool get connected => true;
}

class TierAgent extends LiveAgent {
  final widths = <int>[];
  @override
  Future<void> command(
    String action, [
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
  ]) async {
    if (action == 'thumbnail_size') widths.add(input!['width'] as int);
  }
}

Map<String, dynamic> room(
  List<Map<String, dynamic>> rows, [
  String ip = '10.0.55.200',
]) => {'teacher_ip': ip, 'control_port': 47800, 'students': rows};
Map<String, dynamic> pc(String id, String ip, bool online) => {
  'id': id,
  'name': 'Lab PC',
  'ip': ip,
  'online': online,
};
void main() {
  testWidgets(
    'Thumbnail tiers coalesce slider reversals and suppress repeated tiers',
    (tester) async {
      final a = TierAgent();
      a.thumbnailSize(200);
      await tester.pump(const Duration(milliseconds: 500));
      a.thumbnailSize(450);
      await tester.pump(const Duration(milliseconds: 500));
      a.thumbnailSize(330);
      await tester.pump(const Duration(milliseconds: 899));
      expect(a.widths, isEmpty);
      await tester.pump(const Duration(milliseconds: 1));
      expect(a.widths, [360]);
      a.thumbnailSize(390);
      await tester.pump(const Duration(seconds: 1));
      expect(a.widths, [360]);
      a.thumbnailSize(200);
      await tester.pump(const Duration(milliseconds: 200));
      a.thumbnailSize(330);
      await tester.pump(const Duration(seconds: 1));
      expect(a.widths, [360]);
      a.thumbnailSize(480);
      await tester.pump(const Duration(seconds: 1));
      expect(a.widths, [360, 480]);
      a.dispose();
    },
  );
  test(
    'Reconnect retires matching offline identity, persists migration, preserves live peers',
    () async {
      final shared = FakeStore();
      final a = LiveAgent(shared);
      await a.loadStore();
      a.ingestState(room([pc('old', '10.0.55.2', true)]));
      a.ingestState(
        room([pc('old', '10.0.55.2', false), pc('new', '10.0.55.2', true)]),
      );
      expect(a.students.map((r) => r['id']), ['new']);
      expect(a.seenOnlineIds, {'new'});
      await a.writes;
      final b = LiveAgent(shared);
      await b.loadStore();
      expect(b.students.map((r) => r['id']), ['new']);
      a.ingestState(
        room([pc('new', '10.0.55.2', true), pc('other', '10.0.55.2', true)]),
      );
      expect(a.students.length, 2);
      a.ingestState(
        room([
          pc('new', '10.0.55.2', true),
          pc('different-ip', '10.0.55.3', false),
        ]),
      );
      expect(a.students.any((r) => r['id'] == 'different-ip'), true);
      await a.writes;
      a.dispose();
      b.dispose();
    },
  );
  test(
    'Roster survives restart offline, sorts online then numeric IP, isolates room',
    () async {
      final shared = FakeStore();
      final a = LiveAgent(shared);
      await a.loadStore();
      a.ingestState(
        room([
          pc('b', '10.0.55.10', true),
          pc('a', '10.0.55.2', true),
          pc('c', '10.0.55.1', false),
        ]),
      );
      expect(a.students.map((r) => r['id']), ['a', 'b', 'c']);
      expect(a.seenOnlineIds, {'a', 'b'});
      await a.writes;
      final b = LiveAgent(shared);
      await b.loadStore();
      expect(b.students.length, 3);
      expect(b.students.every((r) => r['online'] == false), true);
      b.ingestState(room([pc('b', '10.0.55.10', true)]));
      expect(b.students.first['id'], 'b');
      expect(b.students.length, 3);
      b.ingestState(room([pc('new', '10.0.63.11', true)], '10.0.63.200'));
      expect(b.students.length, 1);
      await b.writes;
      a.dispose();
      b.dispose();
    },
  );

  test(
    'Real attach order: first state arrives, then the saved roster loads',
    () async {
      final shared = FakeStore();
      final a = LiveAgent(shared);
      await a.loadStore();
      a.ingestState(
        room([pc('x', '10.0.55.5', true), pc('y', '10.0.55.6', false)]),
      );
      await a.writes;
      // Same room: the saved offline Student is restored around the live one.
      final same = LiveAgent(shared);
      same.ingestState(room([pc('x', '10.0.55.5', true)]));
      await same.loadStore();
      expect(same.students.map((r) => r['id']).toSet(), {'x', 'y'});
      // Different room: nothing from the saved room leaks in.
      final other = LiveAgent(shared);
      other.ingestState(room([pc('n', '10.0.63.9', true)], '10.0.63.200'));
      await other.loadStore();
      expect(other.students.map((r) => r['id']), ['n']);
      a.dispose();
      same.dispose();
      other.dispose();
    },
  );
  test('Preferences persist through the settings database', () async {
    final shared = FakeStore();
    final a = LiveAgent(shared);
    await a.loadStore();
    await a.savePreferences({
      'view': 'list',
      'grid_size': 380.0,
      'internet_last': null,
    });
    expect(
      shared.data['prefs'],
      allOf(
        containsPair('view', 'list'),
        containsPair('grid_size', 380.0),
        isNot(contains('internet_last')),
      ),
    );
    final b = LiveAgent(shared);
    await b.loadStore();
    expect(b.preferences['view'], 'list');
    await b.savePreferences({'view': null});
    expect(shared.data['prefs']!.containsKey('view'), false);
    a.dispose();
    b.dispose();
  });
  test(
    'Preferences saved before the agent is reachable are kept and flushed',
    () async {
      final shared = FakeStore();
      final a = LiveAgent(shared);
      await a.savePreferences({'port': 47900});
      expect(a.preferences['port'], 47900);
      expect(shared.data['prefs'], isNull);
      await a.loadStore();
      expect(shared.data['prefs'], containsPair('port', 47900));
      a.dispose();
    },
  );
}
