import 'dart:convert';
import 'dart:io';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/app_icons.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_store.dart';

// A valid 1x1 PNG.
final _png = base64Decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
);

class StoreAgent extends Agent with FakeStoreAgent {
  StoreAgent(this.store) : super([]);
  @override
  final FakeStore store;
}

void main() {
  testWidgets('a site icon is fetched once, kept as a small PNG, and reused', (
    tester,
  ) async {
    // flutter_test answers every HttpClient with 400; this test talks to a
    // real loopback server.
    HttpOverrides.global = null;
    await tester.runAsync(() async {
      final hits = <String>[];
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      server.listen((request) {
        hits.add(request.uri.path);
        if (request.uri.path == '/') {
          request.response
            ..headers.contentType = ContentType.html
            ..write(
              '<html><head><link rel="icon" href="/logo.png"></head></html>',
            );
        } else if (request.uri.path == '/logo.png') {
          request.response
            ..headers.contentType = ContentType('image', 'png')
            ..add(_png);
        } else {
          request.response.statusCode = 404;
        }
        request.response.close();
      });
      final url = 'http://127.0.0.1:${server.port}/some/page';
      final store = FakeStore();

      final first = FaviconCache.create()..bind(StoreAgent(store));
      final icon = await first.forUrl(url);
      expect(icon, isNotNull);
      expect(hits, contains('/logo.png'));
      // Let the background store write finish.
      await Future<void>.delayed(const Duration(milliseconds: 100));
      final saved = await store.list('favicons');
      expect(saved.keys, ['127.0.0.1']);
      expect((saved['127.0.0.1'] as Map)['png'], isA<String>());

      // A new window is a new process: its own empty cache, same store.
      final before = hits.length;
      final second = FaviconCache.create()..bind(StoreAgent(store));
      expect(await second.forUrl(url), isNotNull);
      expect(hits.length, before, reason: 'no request when the icon is stored');

      // A site that has no icon is not stored and does not throw.
      final nothing = FaviconCache.create()..bind(StoreAgent(FakeStore()));
      expect(await nothing.forUrl('http://127.0.0.1:1/'), isNull);
      await server.close(force: true);
    });
  });
}
