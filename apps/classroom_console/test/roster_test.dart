import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';

class LiveAgent extends Agent {
  LiveAgent() : super([]);
  @override
  bool get connected => true;
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
  test(
    'Reconnect retires matching offline identity, persists migration, preserves live peers',
    () async {
      final dir = await Directory.systemTemp.createTemp(
        'classroom-roster-alias',
      );
      final a = LiveAgent()..dataDirectory = dir;
      a.ingestState(room([pc('old', '10.0.55.2', true)]));
      a.ingestState(
        room([pc('old', '10.0.55.2', false), pc('new', '10.0.55.2', true)]),
      );
      expect(a.students.map((r) => r['id']), ['new']);
      expect(a.seenOnlineIds, {'new'});
      await a.writes;
      final b = LiveAgent()..dataDirectory = dir;
      await b.loadKnown();
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
      await dir.delete(recursive: true);
    },
  );
  test(
    'Roster survives restart offline, sorts online then numeric IP, isolates room',
    () async {
      final dir = await Directory.systemTemp.createTemp('classroom-roster');
      final a = LiveAgent()..dataDirectory = dir;
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
      final b = LiveAgent()..dataDirectory = dir;
      await b.loadKnown();
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
      await dir.delete(recursive: true);
    },
  );
}
