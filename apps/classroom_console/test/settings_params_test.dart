import 'dart:io';

import 'package:classroom_console/agent.dart';
import 'package:classroom_console/settings.dart';
import 'package:classroom_console/settings_params.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Reads the `[section] key = value` lines of the shipped defaults.
Map<String, Map<String, Object>> shippedDefaults() {
  final out = <String, Map<String, Object>>{};
  var section = '';
  for (final raw in File('../../config/default.toml').readAsLinesSync()) {
    final line = raw.split('#').first.trim();
    if (line.isEmpty) continue;
    if (line.startsWith('[')) {
      section = line.substring(1, line.length - 1);
      continue;
    }
    final eq = line.indexOf('=');
    if (eq < 0) continue;
    final key = line.substring(0, eq).trim();
    final text = line.substring(eq + 1).trim();
    final Object value = text == 'true'
        ? true
        : text == 'false'
        ? false
        : text.startsWith('"')
        ? text.substring(1, text.length - 1)
        : int.tryParse(text) ?? text;
    (out[section] ??= {})[key] = value;
  }
  return out;
}

class HotAgent extends Agent {
  HotAgent() : super([]);
  @override
  bool get connected => true;
  Map<String, dynamic>? applied;
  @override
  Future<void> applySettings(Map<String, dynamic> overlay) async {
    applied = overlay;
  }

  @override
  Future<void> savePreferences(Map<String, dynamic> changes) async {
    preferences = {...preferences, ...changes};
  }
}

void main() {
  test('every setting defaults to the value the app shipped with', () {
    final shipped = shippedDefaults();
    for (final p in kParams.where((p) => !p.isPref)) {
      expect(
        shipped[p.section]?[p.key],
        p.def,
        reason: '${p.id} default must match config/default.toml',
      );
    }
  });

  test('every setting has a sane range and a label', () {
    for (final p in kParams) {
      expect(p.label.trim(), isNotEmpty, reason: p.id);
      if (p.kind == ParamKind.integer) {
        expect(p.min, isNotNull, reason: p.id);
        expect(p.max, isNotNull, reason: p.id);
        expect(p.def as int, inInclusiveRange(p.min!, p.max!), reason: p.id);
      }
      if (p.kind == ParamKind.choice) {
        expect(p.options!.containsKey(p.def), isTrue, reason: p.id);
      }
    }
    expect(kParams.map((p) => p.id).toSet().length, kParams.length);
  });

  group('ParamStore', () {
    ParamStore store([
      Map<String, dynamic> settings = const {},
      Map<String, dynamic> prefs = const {},
    ]) => ParamStore(settings: settings, preferences: prefs);

    test('starts from the defaults, then from what is saved', () {
      final fresh = store();
      expect(fresh.intValue('recording.fps'), 30);
      expect(fresh.dirty, isFalse);
      final saved = store(
        {
          'recording': {'fps': 15, 'height': 480},
          'activity': {'collect_clipboard': false},
        },
        {'ui_label_mode': 'ip'},
      );
      expect(saved.intValue('recording.fps'), 15);
      expect(saved.value('recording.height'), 480);
      expect(saved.value('activity.collect_clipboard'), false);
      expect(saved.value('pref:ui_label_mode'), 'ip');
      expect(saved.dirty, isFalse, reason: 'opening is not a change');
    });

    test('edits are tracked, reset and validated', () {
      final s = store();
      s.controllers['recording.fps']!.text = '12';
      expect(s.isChanged('recording.fps'), isTrue);
      expect(s.dirty, isTrue);
      expect(s.validate(), isNull);
      s.reset('recording.fps');
      expect(s.intValue('recording.fps'), 30);
      expect(s.isDefault('recording.fps'), isTrue);

      s.controllers['recording.fps']!.text = '99';
      expect(s.validate()!.page, pageRecording);
      s.controllers['recording.fps']!.text = '';
      expect(s.validate(), isNotNull, reason: 'empty is not a number');
      s.reset('recording.fps');

      s.controllers['recording.chunk_minutes']!.text = '30';
      s.controllers['recording.bitrate_kbps']!.text = '12000';
      expect(s.validate()!.message, contains('quá lớn'));
      s.resetAll(['recording.chunk_minutes', 'recording.bitrate_kbps']);

      s.controllers['recording.teacher_warn_mb']!.text = '300';
      expect(s.validate()!.message, contains('cảnh báo'));
      s.reset('recording.teacher_warn_mb');

      s.controllers['recording.gone_minutes']!.text = '3';
      expect(s.validate(), isNotNull);
      s.controllers['recording.gone_minutes']!.text = '0';
      expect(s.validate(), isNull, reason: '0 means never');
      s.reset('recording.gone_minutes');

      s.controllers['recording.folder']!.text = 'GhiHinh';
      expect(s.validate(), isNotNull, reason: 'must be a full path');
      s.controllers['recording.folder']!.text = r'D:\GhiHinh';
      expect(s.validate(), isNull);
      s.controllers['recording.folder']!.text = '/Users/gv/GhiHinh';
      expect(s.validate(), isNull);
    });

    test(
      'config values go to their sections and preferences stay separate',
      () {
        final s = store();
        s.controllers['recording.fps']!.text = '20';
        s.set('pref:ui_label_mode', 'name');
        s.set('activity.collect_clipboard', false);
        final sections = s.configSections();
        expect(sections['recording']!['fps'], 20);
        expect(sections['recording']!['height'], 720);
        expect(sections['activity']!['collect_clipboard'], false);
        expect(sections['tools']!['recording_max_minutes'], 60);
        expect(sections.containsKey('pref:ui_label_mode'), isFalse);
        expect(s.changedPreferences(), {'ui_label_mode': 'name'});
      },
    );

    test('only stream tuning needs a restart', () {
      final s = store();
      s.controllers['recording.fps']!.text = '20';
      s.set('pref:ui_click', 'select');
      expect(s.needsRestart, isFalse);
      s.controllers['video.jitter_buffer_ms']!.text = '60';
      expect(s.needsRestart, isTrue);
    });
  });

  group('Settings dialog', () {
    Future<void> goTo(WidgetTester tester, int page) async {
      final rail = find.byKey(const ValueKey('teacher-settings-sections'));
      final item = find.byKey(ValueKey('settings-page-$page'));
      await tester.scrollUntilVisible(
        item,
        80,
        scrollable: find.descendant(
          of: rail,
          matching: find.byType(Scrollable),
        ),
      );
      await tester.ensureVisible(item);
      await tester.pumpAndSettle();
      await tester.tap(item);
      await tester.pumpAndSettle();
    }

    Future<void> open(WidgetTester tester, HotAgent agent) async {
      tester.view.physicalSize = const Size(1100, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () => showDialog<TeacherSettings>(
                  context: context,
                  builder: (_) => TeacherSettingsDialog(agent: agent),
                ),
                child: const Text('Settings'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Settings'));
      await tester.pumpAndSettle();
    }

    testWidgets('no page name wraps onto two lines in the list', (
      tester,
    ) async {
      final agent = HotAgent()..state = {'control_port': 47800};
      await open(tester, agent);
      final rail = find.byKey(const ValueKey('teacher-settings-sections'));
      for (var i = 0; i < 12; i++) {
        final item = find.byKey(ValueKey('settings-page-$i'));
        await tester.scrollUntilVisible(
          item,
          80,
          scrollable: find.descendant(
            of: rail,
            matching: find.byType(Scrollable),
          ),
        );
        final label = find.descendant(of: item, matching: find.byType(Text));
        expect(
          tester.getSize(label).height,
          lessThan(22),
          reason: 'page $i should fit on one line',
        );
      }
      agent.dispose();
    });

    testWidgets('pages are grouped under headings', (tester) async {
      final agent = HotAgent()..state = {'control_port': 47800};
      await open(tester, agent);
      for (final heading in [
        'LỚP HỌC',
        'MÁY HỌC VIÊN',
        'LƯU & TRUYỀN',
        'HỆ THỐNG',
      ]) {
        final label = find.descendant(
          of: find.byKey(const ValueKey('teacher-settings-sections')),
          matching: find.text(heading),
        );
        await tester.scrollUntilVisible(
          label,
          80,
          scrollable: find.descendant(
            of: find.byKey(const ValueKey('teacher-settings-sections')),
            matching: find.byType(Scrollable),
          ),
        );
        expect(label, findsOneWidget);
      }
      agent.dispose();
    });

    testWidgets('a recording setting is applied at once, no reconnect', (
      tester,
    ) async {
      final agent = HotAgent()..state = {'control_port': 47800};
      await open(tester, agent);
      await goTo(tester, pageRecording);
      await tester.enterText(find.byKey(const ValueKey('recording.fps')), '15');
      await tester.pumpAndSettle();
      expect(find.text('Lưu & áp dụng ngay'), findsOneWidget);
      expect(find.byKey(const ValueKey('reset-recording.fps')), findsOneWidget);
      await tester.tap(find.text('Lưu & áp dụng ngay'));
      await tester.pumpAndSettle();
      expect(agent.applied!['recording']['fps'], 15);
      expect(agent.applied!['recording']['height'], 720);
      expect(
        find.text('Đã áp dụng cài đặt — không cần kết nối lại.'),
        findsOneWidget,
      );
      agent.dispose();
    });

    testWidgets('a stream tuning change still asks to reconnect', (
      tester,
    ) async {
      final agent = HotAgent()..state = {'control_port': 47800};
      await open(tester, agent);
      await goTo(tester, pageStream);
      await tester.scrollUntilVisible(
        find.text('Nâng cao'),
        200,
        scrollable: find.byType(Scrollable).last,
      );
      await tester.tap(find.text('Nâng cao'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('video.jitter_buffer_ms')),
        '60',
      );
      await tester.pumpAndSettle();
      expect(find.text('Lưu & kết nối lại'), findsOneWidget);
      agent.dispose();
    });

    testWidgets(
      'a wrong value is refused on the right page with a clear message',
      (tester) async {
        final agent = HotAgent()..state = {'control_port': 47800};
        await open(tester, agent);
        await goTo(tester, pageRecording);
        await tester.enterText(
          find.byKey(const ValueKey('recording.bitrate_kbps')),
          '999999',
        );
        await tester.pumpAndSettle();
        await tester.tap(find.text('Lưu & áp dụng ngay'));
        await tester.pumpAndSettle();
        expect(agent.applied, isNull);
        expect(
          find.textContaining('nhập số nguyên từ 500 đến 12000'),
          findsOneWidget,
        );
        agent.dispose();
      },
    );

    testWidgets('page reset puts every value back to the default', (
      tester,
    ) async {
      final agent = HotAgent()
        ..state = {
          'control_port': 47800,
          'settings': {
            'recording': {'fps': 12, 'height': 480},
          },
        };
      await open(tester, agent);
      await goTo(tester, pageRecording);
      await tester.scrollUntilVisible(
        find.byKey(const ValueKey('reset-page-10')),
        200,
        scrollable: find.byType(Scrollable).last,
      );
      await tester.tap(find.byKey(const ValueKey('reset-page-10')));
      await tester.pumpAndSettle();
      final field = tester.widget<TextField>(
        find.byKey(const ValueKey('recording.fps')),
      );
      expect(field.controller!.text, '30');
      agent.dispose();
    });
  });
}
