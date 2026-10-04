import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';

void frame(Socket peer, Map<String, dynamic> value) {
  final body = utf8.encode(jsonEncode(value));
  peer.add((ByteData(4)..setUint32(0, body.length)).buffer.asUint8List());
  peer.add(body);
}

void main() {
  for (final observe in [false, true]) {
    test(
      'ordered Exit closes ${observe ? 'Observe' : 'console'} without reconnecting',
      () async {
        final dir = await Directory.systemTemp.createTemp('classroom-exit');
        final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
        final peers = <Socket>[];
        final subscription = server.listen((peer) {
          peers.add(peer);
          peer.listen((_) {});
          frame(peer, {
            'type': 'state',
            'lifecycle': 'running',
            'students': [],
          });
        });
        final endpoint = File('${dir.path}/endpoint.json');
        await endpoint.writeAsString(
          jsonEncode({'port': server.port, 'token': 'test'}),
        );
        var closed = 0;
        final agent = Agent(
          observe ? ['--observe-id', 'student'] : [],
          closeApplication: () async {
            closed++;
          },
        )..endpoint = endpoint;
        try {
          await agent.attach();
          frame(peers.single, {
            'type': 'state',
            'lifecycle': 'exit',
            'students': [],
          });
          await peers.single.flush();
          await peers.single.close();
          await Future<void>.delayed(const Duration(milliseconds: 100));
          expect(closed, 1);
          expect(agent.exiting, true);
          expect(agent.shutdownReceived, true);
          expect(agent.reconnect?.isActive ?? false, false);
          await agent.finishApplication();
          expect(closed, 1);
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
}
