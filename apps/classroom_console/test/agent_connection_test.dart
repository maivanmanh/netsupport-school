import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';

void main() {
  test(
    'closing old IPC socket cannot disconnect its replacement roster',
    () async {
      final dir = await Directory.systemTemp.createTemp(
        'classroom-ipc-replace',
      );
      final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
      final peers = <Socket>[];
      final subscription = server.listen((peer) {
        peers.add(peer);
        peer.listen((_) {});
        final body = utf8.encode(
          jsonEncode({
            'type': 'state',
            'teacher_ip': '127.0.0.1',
            'control_port': 47800,
            'students': [
              {
                'id': 'student',
                'name': 'PC',
                'ip': '127.0.0.2',
                'online': true,
              },
            ],
          }),
        );
        peer.add((ByteData(4)..setUint32(0, body.length)).buffer.asUint8List());
        peer.add(body);
      });
      final endpoint = File('${dir.path}/endpoint.json');
      await endpoint.writeAsString(
        jsonEncode({'port': server.port, 'token': 'test'}),
      );
      final agent = Agent([])..endpoint = endpoint;
      // Do not launch a real agent if the fake test endpoint closes.
      agent.exiting = true;
      try {
        await agent.attach();
        await agent.attach();
        final replacement = agent.socket;
        peers.first.destroy();
        await Future<void>.delayed(const Duration(milliseconds: 50));
        expect(agent.socket, same(replacement));
        expect(agent.connected, isTrue);
        expect(agent.students.single['online'], isTrue);
      } finally {
        agent.dispose();
        for (final peer in peers) {
          peer.destroy();
        }
        await subscription.cancel();
        await server.close();
        await dir.delete(recursive: true);
      }
    },
  );
}
