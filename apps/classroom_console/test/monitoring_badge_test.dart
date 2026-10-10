import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/monitoring_badge.dart';
void main() {
  test('badge reflects acknowledgement, offline retention and errors', () {
    final agent = Agent([]);
    void state(Map value) => agent.state = {'tools': {'monitoring_badges': {'pc': value}}};
    expect(badgeLabel(agent, 'pc'), 'Đã tắt');
    state({'enabled': true, 'status': 'pending'});
    expect(badgeLabel(agent, 'pc'), 'Đang bật…');
    state({'enabled': true, 'actual': true, 'status': 'applied'});
    expect(badgeLabel(agent, 'pc'), contains('đã xác nhận'));
    state({'enabled': true, 'status': 'offline'});
    expect(badgeLabel(agent, 'pc'), contains('Đang bật'));
    state({'enabled': true, 'status': 'timeout', 'error': 'No display'});
    expect(badgeLabel(agent, 'pc'), contains('No display'));
    state({'enabled': false, 'actual': false, 'status': 'applied'});
    expect(badgeLabel(agent, 'pc'), 'Đã tắt • đã xác nhận');
    agent.dispose();
  });
}
