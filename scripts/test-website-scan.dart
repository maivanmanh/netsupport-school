// Real Chromium integration fixture; no internet required.
import 'dart:io';
import 'dart:convert';
import '../apps/classroom_console/lib/website_dependencies.dart';

Future<void> main() async {
  final server = await HttpServer.bind(InternetAddress.anyIPv4, 0);
  var iframeHits = 0, navigationHits = 0, postHits = 0;
  final port = server.port;
  final pending = <Future<void>>[];
  server.listen((r) {
    pending.add(() async {
      try {
        if (r.uri.path == '/redirect') {
          r.response.statusCode = 302;
          r.response.headers.set('location', 'http://localhost:$port/page');
        } else if (r.uri.path == '/page') {
          r.response.headers.contentType = ContentType.html;
          r.response.write(
            '''<link rel="stylesheet" href="http://127.0.0.1:$port/style.css">
          <script src="http://127.0.0.1:$port/code.js"></script>
          <iframe src="http://127.0.0.3:$port/frame"></iframe>
          <div style="height:4000px">fixture</div>''',
          );
        } else if (r.uri.path == '/style.css') {
          r.response.headers.set('Content-Type', 'text/css');
          r.response.write('body { color:red; }');
        } else if (r.uri.path == '/code.js') {
          r.response.headers.set('Content-Type', 'application/javascript');
          r.response.write(
            "fetch('http://127.0.0.2:$port/api'); fetch('http://127.0.0.6:$port/write',{method:'POST',body:'test'}); setTimeout(() => location.href='http://127.0.0.5:$port/escape', 500);",
          );
        } else if (r.uri.path == '/write') {
          postHits++;
          r.response.write('{}');
        } else if (r.uri.path == '/frame') {
          iframeHits++;
          r.response.headers.contentType = ContentType.html;
          r.response.write('iframe');
        } else if (r.uri.path == '/escape') {
          navigationHits++;
          r.response.write('escape');
        } else if (r.uri.path == '/slow') {
          r.response.headers.contentType = ContentType.html;
          r.response.write(
            "<img src='http://127.0.0.1:$port/image.png'><script>setInterval(()=>{new Image().src='http://127.0.0.1:$port/image.png?t='+Date.now()},200)</script>",
          );
        } else if (r.uri.path == '/hang') {
          await Future<void>.delayed(const Duration(seconds: 7));
        } else if (r.uri.path == '/image.png') {
          r.response.headers.set('content-type', 'image/png');
          r.response.add(
            base64Decode(
              'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aKZcAAAAASUVORK5CYII=',
            ),
          );
        } else {
          r.response.headers.set('access-control-allow-origin', '*');
          r.response.write('{}');
        }
        await r.response.close();
      } catch (_) {}
    }());
  });
  try {
    final result = await WebsiteScanner().scan(
      'http://localhost:$port/redirect',
    );
    final static = result.dependencies.singleWhere(
      (d) => d.host == '127.0.0.1',
    );
    if (!static.allowed || !static.types.containsAll(['Stylesheet', 'Script']))
      throw StateError('Missing static dependencies');
    if (result.dependencies.singleWhere((d) => d.host == '127.0.0.2').allowed)
      throw StateError('API auto-opened');
    if (iframeHits != 0 || navigationHits != 0 || postHits != 0)
      throw StateError(
        'Visited iframe/JS navigation/POST: $iframeHits/$navigationHits/$postHits',
      );
    final watch = Stopwatch()..start();
    final timed = await WebsiteScanner().scan(
      'http://localhost:$port/slow',
      budget: const Duration(seconds: 3),
    );
    if (timed.status != 'timeout' || !timed.dependencies.any((d) => d.allowed))
      throw StateError(
        'Timeout lost partial dependencies: ${timed.status} ${timed.dependencies.map((d) => d.toJson()).toList()}',
      );
    if (watch.elapsed > const Duration(seconds: 7))
      throw StateError('Exceeded bounded cleanup');
    final cancel = WebsiteScanner();
    final future = cancel.scan('http://localhost:$port/slow');
    await Future<void>.delayed(const Duration(seconds: 1));
    cancel.cancel();
    final cancelled = await future;
    if (cancelled.status != 'cancelled') throw StateError('Cancel failed');
    print(
      jsonEncode({
        'fixture': 'passed',
        'static': static.toJson(),
        'iframe_requests': iframeHits,
        'external_navigation_requests': navigationHits,
        'post_requests': postHits,
        'timeout': timed.status,
        'timeout_ms': watch.elapsedMilliseconds,
        'partial_domains': timed.dependencies.map((d) => d.host).toList(),
        'cancel': cancelled.status,
      }),
    );
  } finally {
    await server.close(force: true);
    await Future.wait(pending);
  }
}
