// Teacher-only browser inspection. No renderer/server/UI is embedded in product.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

String websiteHost(String value) {
  final uri = Uri.tryParse(value.contains('://') ? value : 'https://$value');
  return (uri?.host ?? '').toLowerCase().replaceFirst(RegExp(r'\.$'), '');
}

Uri websiteUri(String value) {
  final uri = Uri.tryParse(
    value.contains('://') ? value.trim() : 'https://${value.trim()}',
  );
  if (uri == null ||
      !['http', 'https'].contains(uri.scheme) ||
      uri.host.isEmpty ||
      uri.userInfo.isNotEmpty ||
      uri.host.contains('*') ||
      !RegExp(r'^[a-zA-Z0-9._-]+$').hasMatch(uri.host)) {
    throw const FormatException(
      'Enter an HTTP(S) website, without wildcards or credentials.',
    );
  }
  return uri.replace(path: uri.path.isEmpty ? '/' : uri.path).removeFragment();
}

class WebsiteDependency {
  WebsiteDependency(
    this.host, {
    Iterable<String> types = const [],
    this.allowed = false,
  }) : types = types.toSet();
  final String host;
  final Set<String> types;
  bool allowed;
  factory WebsiteDependency.fromJson(Map j) => WebsiteDependency(
    '${j['host']}',
    types: (j['types'] as List? ?? []).whereType<String>(),
    allowed: j['allowed'] == true,
  );
  Map<String, dynamic> toJson() => {
    'host': host,
    'types': types.toList()..sort(),
    'allowed': allowed,
  };
}

class WebsiteGroup {
  WebsiteGroup(
    this.url, {
    this.scannedAt,
    this.status = 'not_scanned',
    this.note = '',
    List<WebsiteDependency>? dependencies,
  }) : dependencies = dependencies ?? [];
  final String url;
  String get host => websiteHost(url);
  DateTime? scannedAt;
  String status, note;
  List<WebsiteDependency> dependencies;
  factory WebsiteGroup.fromJson(Map j) => WebsiteGroup(
    '${j['url']}',
    scannedAt: DateTime.tryParse('${j['scanned_at']}'),
    status: '${j['status'] ?? 'not_scanned'}',
    note: '${j['note'] ?? ''}',
    dependencies: [
      for (final d in j['dependencies'] as List? ?? [])
        if (d is Map) WebsiteDependency.fromJson(d),
    ],
  );
  Map<String, dynamic> toJson() => {
    'url': url,
    'scanned_at': scannedAt?.toUtc().toIso8601String(),
    'status': status,
    'note': note,
    'dependencies': dependencies.map((d) => d.toJson()).toList(),
  };
}

/// Recompute the union; removing one group never removes a shared dependency
/// still required by another. Group rules only contribute to allow-only mode.
List<String> effectiveWebsiteRules(
  String manual,
  Iterable<WebsiteGroup> groups,
) => {
  for (final line in manual.split('\n'))
    if (line.trim().isNotEmpty) line.trim(),
  for (final g in groups) ...[
    g.host,
    for (final d in g.dependencies)
      if (d.allowed) d.host,
  ],
}.toList();

bool knownTrackingHost(String host) {
  if (RegExp(
    r'(^|[.-])(ad|ads|advert|advertising|tracker|tracking|analytics|telemetry|beacon|pixel|metrics|tagmanager)([.-]|$)',
  ).hasMatch(host)) {
    return true;
  }
  return const [
    'google-analytics.com',
    'googletagmanager.com',
    'doubleclick.net',
    'googlesyndication.com',
    'googleadservices.com',
    'facebook.net',
    'connect.facebook.com',
    'hotjar.com',
    'hotjar.io',
    'clarity.ms',
    'segment.io',
    'segment.com',
    'sentry.io',
    'adservice.google.com',
    'amazon-adsystem.com',
    'adsrvr.org',
    'adnxs.com',
    'criteo.com',
    'taboola.com',
    'outbrain.com',
    'app-measurement.com',
  ].any((d) => host == d || host.endsWith('.$d'));
}

const _staticTypes = {'Stylesheet', 'Script', 'Image', 'Font', 'Manifest'};

/// Collect only main-frame successful static responses. Type and MIME must
/// agree. Iframes, failed loads, tracking, API and sockets never auto-open hosts.
class DependencyCollector {
  DependencyCollector(this.root);
  final String root;
  final Map<String, WebsiteDependency> entries = {};
  bool capped = false;
  void observe(
    String url,
    String type, {
    bool success = false,
    String mime = '',
    bool mainFrame = true,
  }) {
    if (!mainFrame) return;
    final uri = Uri.tryParse(url);
    if (uri == null || !['http', 'https', 'ws', 'wss'].contains(uri.scheme)) {
      return;
    }
    final host = uri.host.toLowerCase();
    if (host.isEmpty || host == root || host.endsWith('.$root')) return;
    if (entries.length >= 64 && !entries.containsKey(host)) {
      capped = true;
      return;
    }
    final tracking = knownTrackingHost(host);
    final d = entries.putIfAbsent(host, () => WebsiteDependency(host));
    d.types.add(tracking ? 'Tracking (excluded)' : type);
    final validMime = switch (type) {
      'Stylesheet' => mime == 'text/css',
      'Script' => mime.contains('javascript') || mime.contains('ecmascript'),
      'Image' => mime.startsWith('image/'),
      'Font' =>
        mime.startsWith('font/') ||
            mime.contains('font') ||
            mime.contains('opentype'),
      'Manifest' => mime.contains('json') || mime.contains('manifest'),
      'Redirect' => true,
      _ => false,
    };
    if (!tracking &&
        success &&
        validMime &&
        (_staticTypes.contains(type) || type == 'Redirect')) {
      d.allowed = true;
    }
  }
}

class ScanResult {
  ScanResult(this.dependencies, this.status, this.note);
  final List<WebsiteDependency> dependencies;
  final String status, note;
}

class WebsiteScanner {
  Process? _process;
  _DevTools? _cdp;
  bool _cancelled = false;
  void cancel() {
    _cancelled = true;
    unawaited(_cdp?.close());
  }

  static Future<String?> findBrowser([String? configured]) async {
    if (configured != null && configured.trim().isNotEmpty) {
      if (await File(configured.trim()).exists()) return configured.trim();
      throw const FileSystemException(
        'Configured browser executable was not found.',
      );
    }
    final candidates = <String>[];
    if (Platform.isMacOS) {
      for (final base in [
        '/Applications',
        '${Platform.environment['HOME']}/Applications',
      ]) {
        for (final app in ['Google Chrome', 'Microsoft Edge', 'Chromium']) {
          candidates.add('$base/$app.app/Contents/MacOS/$app');
        }
      }
    } else if (Platform.isWindows) {
      for (final key in ['PROGRAMFILES', 'PROGRAMFILES(X86)', 'LOCALAPPDATA']) {
        // Windows environment variable names are case-insensitive, but the
        // Dart map may preserve their original spelling.
        final base = Platform.environment.entries
            .where((entry) => entry.key.toUpperCase() == key)
            .map((entry) => entry.value)
            .firstOrNull;
        if (base == null) continue;
        for (final suffix in [
          r'Google\Chrome\Application\chrome.exe',
          r'Microsoft\Edge\Application\msedge.exe',
          r'Chromium\Application\chrome.exe',
        ]) {
          candidates.add('$base${Platform.pathSeparator}$suffix');
        }
      }
    } else {
      candidates.addAll([
        '/usr/bin/google-chrome',
        '/usr/bin/chromium',
        '/usr/bin/chromium-browser',
      ]);
    }
    for (final path in candidates) {
      if (await File(path).exists()) return path;
    }
    return null;
  }

  Future<ScanResult> scan(
    String value, {
    String? browserPath,
    Duration budget = const Duration(seconds: 15),
  }) async {
    final url = websiteUri(value);
    if (budget < const Duration(seconds: 3) ||
        budget > const Duration(seconds: 30)) {
      throw const FormatException('Scan timeout must be 3–30 seconds.');
    }
    final collector = DependencyCollector(url.host.toLowerCase());
    Directory? profile;
    String
    status = 'scanned',
    note =
        'Page scanned; interactive or signed-in content may need additional domains.';
    try {
      final browser = await findBrowser(browserPath);
      if (browser == null) {
        throw StateError(
          'Chrome or Edge is required to scan. Install one or configure its executable path. Saved groups still work without a browser.',
        );
      }
      profile = await Directory.systemTemp.createTemp('classroom-site-scan-');
      await _run(browser, profile, url, collector).timeout(budget);
      if (_cancelled) {
        status = 'cancelled';
        note = 'Scan stopped. Partial results retained.';
      } else if (collector.capped) {
        status = 'partial';
        note = 'Domain/request limit reached; partial results retained.';
      }
    } on TimeoutException {
      status = 'timeout';
      note =
          'Time limit reached; partial results retained. No automatic rescan.';
    } catch (_) {
      if (!_cancelled) rethrow;
      status = 'cancelled';
      note = 'Scan stopped. Partial results retained.';
    } finally {
      _cancelled = true;
      final cdp = _cdp;
      if (cdp != null) {
        try {
          await cdp
              .call('Browser.close')
              .timeout(const Duration(milliseconds: 700));
        } catch (_) {}
        await cdp.close();
      }
      final process = _process;
      if (process != null) {
        if (Platform.isWindows) {
          // Only this scanner's private browser tree, never a user's browser.
          try {
            await Process.run('taskkill', [
              '/PID',
              '${process.pid}',
              '/T',
              '/F',
            ]).timeout(const Duration(seconds: 2));
          } catch (_) {}
        } else {
          process.kill();
        }
        try {
          await process.exitCode.timeout(const Duration(seconds: 2));
        } catch (_) {
          process.kill(ProcessSignal.sigkill);
        }
      }
      if (profile != null) {
        try {
          await profile.delete(recursive: true);
        } catch (_) {}
      }
    }
    return ScanResult(
      collector.entries.values.toList()
        ..sort((a, b) => a.host.compareTo(b.host)),
      status,
      note,
    );
  }

  Future<void> _run(
    String browser,
    Directory profile,
    Uri url,
    DependencyCollector collector,
  ) async {
    _process = await Process.start(browser, [
      '--headless=new',
      '--remote-debugging-address=127.0.0.1',
      '--remote-debugging-port=0',
      '--user-data-dir=${profile.path}',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-extensions',
      '--disable-background-networking',
      '--disable-sync',
      '--mute-audio',
      '--window-size=1366,900',
      'about:blank',
    ]);
    // Drain pipes without retaining unbounded browser logs.
    _process!.stdout.listen((_) {});
    _process!.stderr.listen((_) {});
    final active = File('${profile.path}/DevToolsActivePort');
    final startup = Stopwatch()..start();
    while (!await active.exists()) {
      if (_cancelled) return;
      if (startup.elapsed > const Duration(seconds: 5)) {
        throw StateError(
          'The headless browser did not start within 5 seconds. Check the configured browser.',
        );
      }
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    final lines = await active.readAsLines();
    final port = int.parse(lines.first);
    if (_cancelled) return;
    final cdp = _DevTools(
      await WebSocket.connect(
        'ws://127.0.0.1:$port${lines[1]}',
      ).timeout(const Duration(seconds: 2)),
    );
    _cdp = cdp;
    final target = await cdp.call('Target.createTarget', {
      'url': 'about:blank',
    });
    final attached = await cdp.call('Target.attachToTarget', {
      'targetId': target['targetId'],
      'flatten': true,
    });
    final session = attached['sessionId'] as String;
    Future<Map> call(String method, [Map<String, dynamic> params = const {}]) =>
        cdp.call(method, params, session);
    await call('Page.enable');
    final tree = await call('Page.getFrameTree');
    final frame = tree['frameTree']['frame']['id'];
    await call('Network.enable', {
      'maxTotalBufferSize': 1024 * 1024,
      'maxResourceBufferSize': 128 * 1024,
    });
    await call('Network.setCacheDisabled', {'cacheDisabled': true});
    await call('Network.setBypassServiceWorker', {'bypass': true});
    await cdp.call('Browser.setDownloadBehavior', {'behavior': 'deny'});
    await call('Fetch.enable', {
      'patterns': [
        {'urlPattern': '*', 'requestStage': 'Request'},
      ],
    });
    final navigations = <String>{};
    int requests = 0, bytes = 0, redirects = 0;
    bool loaded = false;
    var lastActivity = DateTime.now();
    cdp.onEvent = (event) {
      if (_cancelled || event['sessionId'] != session) return;
      final p = event['params'] as Map? ?? {};
      final method = event['method'];
      if (method == 'Page.loadEventFired' ||
          method == 'Page.domContentEventFired') {
        loaded = true;
      }
      if (method == 'Network.dataReceived') {
        bytes += (p['dataLength'] as num? ?? 0).toInt();
      }
      if (requests > 2000 || bytes > 30 * 1024 * 1024) {
        collector.capped = true;
      }
      if (method == 'Fetch.requestPaused') {
        lastActivity = DateTime.now();
        requests++;
        final request = p['request'] as Map;
        final requestUrl = '${request['url']}';
        final type = '${p['resourceType']}';
        final id = '${p['requestId']}';
        final host = websiteHost(requestUrl);
        final main = p['frameId'] == frame;
        bool allow =
            main &&
            !collector.capped &&
            !knownTrackingHost(host) &&
            ['GET', 'HEAD'].contains(request['method']);
        if (type == 'Document') {
          final redirect = navigations.contains(p['redirectedRequestId']);
          final safeRedirect =
              host == url.host.toLowerCase() ||
              host.endsWith('.${url.host.toLowerCase()}');
          allow &=
              requestUrl == url.toString() ||
              (redirect && redirects < 5 && safeRedirect);
          if (allow) {
            navigations.add(id);
            if (redirect) {
              redirects++;
              collector.observe(requestUrl, 'Redirect', success: true);
            }
          }
        }
        if (main && type == 'Document') {
          collector.observe(requestUrl, 'Navigation (excluded)');
        }
        // Observe API requests for review, but never auto-grant their domains.
        unawaited(
          call(allow ? 'Fetch.continueRequest' : 'Fetch.failRequest', {
            'requestId': id,
            if (!allow) 'errorReason': 'BlockedByClient',
          }).catchError((_) => <String, dynamic>{}),
        );
      }
      if (method == 'Network.requestWillBeSent') {
        final request = p['request'] as Map? ?? {};
        final type = '${p['type']}';
        final main = p['frameId'] == frame;
        if (main &&
            ['XHR', 'Fetch', 'WebSocket', 'EventSource'].contains(type)) {
          collector.observe('${request['url']}', type);
        }
      }
      if (method == 'Network.responseReceived') {
        lastActivity = DateTime.now();
        final response = p['response'] as Map;
        final code = response['status'] as num? ?? 0;
        collector.observe(
          '${response['url']}',
          '${p['type']}',
          success: code >= 200 && code < 300,
          mime: '${response['mimeType']}',
          mainFrame: p['frameId'] == frame,
        );
      }
      if (method == 'Network.webSocketCreated') {
        collector.observe('${p['url']}', 'WebSocket');
      }
    };
    final navigation = await call('Page.navigate', {'url': url.toString()});
    if (navigation['errorText'] != null) {
      throw StateError('Page could not load: ${navigation['errorText']}');
    }
    final elapsed = Stopwatch()..start();
    int scrolls = 0;
    while (!_cancelled && !collector.capped) {
      await Future<void>.delayed(const Duration(milliseconds: 200));
      if (_cancelled) break;
      if (loaded &&
          scrolls < 3 &&
          elapsed.elapsedMilliseconds >= (scrolls + 1) * 800) {
        scrolls++;
        await call('Runtime.evaluate', {
          'expression': 'window.scrollBy(0, Math.max(600, window.innerHeight))',
        });
      }
      if (loaded &&
          scrolls == 3 &&
          DateTime.now().difference(lastActivity) >
              const Duration(seconds: 2)) {
        break;
      }
    }
  }
}

class _DevTools {
  _DevTools(this.socket) {
    socket.listen(
      (message) {
        if (message is! String || message.length > 2 * 1024 * 1024) return;
        final event = jsonDecode(message) as Map;
        final completer = pending.remove(event['id']);
        if (completer != null) {
          if (event['error'] != null) {
            completer.completeError(StateError('${event['error']}'));
          } else {
            completer.complete(event['result'] as Map? ?? {});
          }
        } else {
          onEvent?.call(event);
        }
      },
      onError: (Object e) => fail(e),
      onDone: () => fail(StateError('Browser connection closed.')),
    );
  }
  final WebSocket socket;
  int counter = 0;
  final pending = <int, Completer<Map>>{};
  void Function(Map)? onEvent;
  Future<Map> call(
    String method, [
    Map<String, dynamic> params = const {},
    String? session,
  ]) async {
    final id = ++counter;
    final done = Completer<Map>();
    pending[id] = done;
    socket.add(
      jsonEncode({
        'id': id,
        'method': method,
        'params': params,
        'sessionId': ?session,
      }),
    );
    try {
      return await done.future.timeout(const Duration(seconds: 3));
    } finally {
      pending.remove(id);
    }
  }

  void fail(Object error) {
    for (final c in pending.values) {
      if (!c.isCompleted) c.completeError(error);
    }
    pending.clear();
  }

  Future<void> close() async {
    onEvent = null;
    await socket.close().timeout(const Duration(seconds: 1), onTimeout: () {});
  }
}
