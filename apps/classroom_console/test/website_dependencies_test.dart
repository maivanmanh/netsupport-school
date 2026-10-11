import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/website_dependencies.dart';
import 'package:classroom_console/internet_access.dart';

void main() {
  test(
    'URLs normalize without turning wildcards, credentials or file URLs into scans',
    () {
      expect(websiteUri('EXAMPLE.com').toString(), 'https://example.com/');
      expect(
        websiteUri('https://example.com/code?q=1#part').toString(),
        'https://example.com/code?q=1',
      );
      for (final input in [
        '*.example.com',
        'file:///tmp/test',
        'https://user:pass@example.com',
        'data:text/html,a',
      ]) {
        expect(() => websiteUri(input), throwsFormatException);
      }
    },
  );
  test(
    'successful main-frame static types auto-allow; other observed hosts need review',
    () {
      final c = DependencyCollector('example.com');
      for (final type in {
        'Stylesheet': 'text/css',
        'Script': 'application/javascript',
        'Image': 'image/png',
        'Font': 'font/woff2',
        'Manifest': 'application/manifest+json',
      }.entries) {
        c.observe(
          'https://${type.key.toLowerCase()}.cdn.test/a',
          type.key,
          success: true,
          mime: type.value,
        );
      }
      c.observe(
        'https://api.test/v1',
        'XHR',
        success: true,
        mime: 'application/json',
      );
      c.observe('https://socket.test/', 'WebSocket');
      c.observe(
        'https://wrong.test/',
        'Script',
        success: true,
        mime: 'text/html',
      );
      c.observe('https://failed.test/', 'Stylesheet', mime: 'text/css');
      c.observe(
        'https://ads.example.net/creative.js',
        'Script',
        success: true,
        mime: 'application/javascript',
      );
      c.observe(
        'https://google-analytics.com/a.js',
        'Script',
        success: true,
        mime: 'application/javascript',
      );
      c.observe(
        'https://iframe.test/a.css',
        'Stylesheet',
        success: true,
        mime: 'text/css',
        mainFrame: false,
      );
      c.observe(
        'https://sub.example.com/a.js',
        'Script',
        success: true,
        mime: 'application/javascript',
      );
      expect(c.entries.values.where((d) => d.allowed), hasLength(5));
      for (final host in [
        'api.test',
        'socket.test',
        'wrong.test',
        'failed.test',
        'google-analytics.com',
        'ads.example.net',
      ]) {
        expect(c.entries[host]!.allowed, isFalse);
      }
      expect(c.entries.containsKey('iframe.test'), isFalse);
      expect(c.entries.containsKey('sub.example.com'), isFalse);
    },
  );
  test(
    'groups persist complete results and deduplicate shared dependencies when removed',
    () {
      final shared = WebsiteDependency(
        'cdn.test',
        types: ['Font', 'Stylesheet'],
        allowed: true,
      );
      final a = WebsiteGroup(
        'https://a.test/',
        status: 'timeout',
        scannedAt: DateTime.utc(2026),
        note: 'partial',
        dependencies: [shared],
      );
      final b = WebsiteGroup(
        'https://b.test/',
        dependencies: [WebsiteDependency.fromJson(shared.toJson())],
      );
      final list = SiteList(
        id: '1',
        name: 'Exam',
        rules: 'manual.test\n# note',
        groups: [a, b],
      );
      final restored = SiteList.fromJson(jsonDecode(jsonEncode(list.toJson())));
      expect(restored.groups[0].scannedAt, DateTime.utc(2026));
      expect(restored.groups[0].status, 'timeout');
      expect(restored.groups[0].dependencies.single.types, {
        'Font',
        'Stylesheet',
      });
      expect(
        effectiveWebsiteRules(
          restored.rules,
          restored.groups,
        ).where((s) => s == 'cdn.test'),
        hasLength(1),
      );
      restored.groups.removeAt(0);
      expect(
        effectiveWebsiteRules(restored.rules, restored.groups),
        contains('cdn.test'),
      );
      restored.groups.clear();
      expect(
        effectiveWebsiteRules(restored.rules, restored.groups),
        isNot(contains('cdn.test')),
      );
      // Legacy saved lists are read unchanged.
      expect(
        SiteList.fromJson({
          'id': 'old',
          'name': 'old',
          'rules': '*.edu.vn',
        }).groups,
        isEmpty,
      );
    },
  );
  test('observed domain storage is bounded', () {
    final c = DependencyCollector('example.com');
    for (var i = 0; i < 100; i++) {
      c.observe(
        'https://cdn$i.test/a',
        'Image',
        success: true,
        mime: 'image/png',
      );
    }
    expect(c.entries.length, 64);
    expect(c.capped, isTrue);
  });
}
