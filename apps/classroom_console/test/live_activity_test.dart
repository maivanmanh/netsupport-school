import 'package:classroom_console/live_activity.dart';
import 'package:classroom_console/recording.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('live activity data', () {
    final tools = {
      'activity_monitoring': true,
      'activity_live': {
        's1': {
          'apps': [
            {'app': 'chrome.exe', 'title': 'Docs'},
          ],
          'url': 'https://www.docs.rs/crate/x?q=1',
        },
        's2': {'apps': []},
      },
    };

    test('only shown while monitoring is on and the Student has apps', () {
      expect(liveActivityFor(tools, 's1'), isNotNull);
      expect(liveActivityFor(tools, 's2'), isNull);
      expect(liveActivityFor(tools, 'missing'), isNull);
      expect(
        liveActivityFor({...tools, 'activity_monitoring': false}, 's1'),
        isNull,
      );
      expect(liveActivityFor(const {}, 's1'), isNull);
    });

    test('addresses are shortened for tiles', () {
      expect(shortUrl('https://www.docs.rs/crate/x?q=1'), 'docs.rs');
      expect(
        shortUrl('https://www.docs.rs/crate/x', pathChars: 40),
        'docs.rs/crate/x',
      );
      expect(shortUrl('https://a.dev/abcdefghij', pathChars: 4), 'a.dev/abc…');
      expect(shortUrl('https://a.dev/', pathChars: 10), 'a.dev');
      expect(shortUrl('edge://newtab'), 'edge://newtab');
    });

    test('AI hosts come from the summary the Teacher publishes', () {
      expect(
        liveAiHosts({
          'ai': ['chatgpt.com', 'claude.ai'],
        }),
        ['chatgpt.com', 'claude.ai'],
      );
      expect(liveAiHosts({'apps': []}), isEmpty);
      expect(liveAiHosts(null), isEmpty);
    });

    Future<void> pumpLive(
      WidgetTester tester,
      Map summary,
      LiveLayout layout, {
      double width = 300,
    }) => tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(
            child: SizedBox(
              width: width,
              child: LiveActivity(summary: summary, layout: layout),
            ),
          ),
        ),
      ),
    );

    testWidgets('every layout copes with little, some and a lot of information', (
      tester,
    ) async {
      final little = {
        'apps': [
          {'app': 'notepad.exe', 'title': 'todo'},
        ],
      };
      final lots = {
        'apps': [
          {'app': 'chrome.exe', 'title': 'Docs'},
          {'app': 'Code.exe', 'title': 'main.rs'},
          {'app': 'notepad.exe', 'title': 'todo'},
        ],
        'url':
            'https://docs.google.com/document/d/1234567890abcdefghijklmnopqrstuvwxyz',
        'url_app': 'chrome.exe',
        'url_in_front': false,
        'ai': ['chatgpt.com', 'claude.ai', 'gemini.google.com'],
      };
      for (final layout in LiveLayout.values) {
        for (final width in [180.0, 300.0, 700.0]) {
          for (final summary in [little, lots]) {
            await pumpLive(tester, summary, layout, width: width);
            expect(tester.takeException(), isNull, reason: '$layout $width');
          }
        }
      }
    });

    testWidgets('the AI warning is shown first and names the sites', (
      tester,
    ) async {
      await pumpLive(tester, {
        'apps': [
          {'app': 'Code.exe', 'title': 'main.rs'},
        ],
        'url': 'https://docs.rs/x',
        'url_in_front': false,
        'ai': ['chatgpt.com', 'claude.ai'],
      }, LiveLayout.grid);
      expect(find.text('AI · chatgpt.com  +1'), findsOneWidget);
      // The page of the background browser is still there, under the warning.
      expect(find.text('docs.rs'), findsOneWidget);
      expect(
        find.byTooltip('Đang mở website AI: chatgpt.com, claude.ai'),
        findsOneWidget,
      );
      expect(
        find.byTooltip('https://docs.rs/x\n(trình duyệt đang chạy nền)'),
        findsOneWidget,
      );
    });

    testWidgets(
      'the page is not repeated when it is the flagged AI site itself',
      (tester) async {
        await pumpLive(tester, {
          'apps': [
            {'app': 'chrome.exe', 'title': 'ChatGPT'},
          ],
          'url': 'https://chatgpt.com/c/1',
          'ai': ['chatgpt.com'],
        }, LiveLayout.grid);
        expect(find.text('AI · chatgpt.com'), findsOneWidget);
        expect(find.text('chatgpt.com'), findsNothing);
      },
    );

    testWidgets('shows an icon per app and the host of the active tab', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 300,
              child: LiveActivity(
                layout: LiveLayout.grid,
                summary: {
                  'apps': [
                    {'app': 'chrome.exe', 'title': 'Docs'},
                    {'app': 'Code.exe', 'title': 'main.rs'},
                    {'app': 'notepad.exe', 'title': 'todo'},
                  ],
                  'url': 'https://www.docs.rs/crate/x',
                },
              ),
            ),
          ),
        ),
      );
      expect(find.text('docs.rs'), findsOneWidget);
      expect(find.byType(Tooltip), findsNWidgets(4));
    });
  });

  group('recording status marks', () {
    Map session({
      String phase = 'stopped',
      bool stop = false,
      String reason = '',
      int queued = 0,
      int completed = 1,
      int received = 1,
    }) => {
      'stop': stop,
      'stop_reason': reason,
      'targets': [
        {
          'id': 'a',
          'received': received,
          'report': {
            'phase': phase,
            'queued_chunks': queued,
            'completed_chunks': completed,
          },
        },
      ],
    };

    test('each outcome has its own icon, colour and tooltip text', () {
      final done = recordingSessionStatus(session());
      expect(done.icon, Icons.check_circle);
      expect(done.label, 'Đã hoàn tất');

      final live = recordingSessionStatus(session(phase: 'recording'));
      expect(live.live, isTrue);
      expect(live.label, 'Đang ghi');

      final stopping = recordingSessionStatus(
        session(phase: 'recording', stop: true),
      );
      expect(stopping.live, isFalse);
      expect(stopping.label, 'Đang dừng');

      final uploading = recordingSessionStatus(session(queued: 1));
      expect(uploading.icon, Icons.cloud_upload_outlined);

      final failed = recordingSessionStatus(session(phase: 'error'));
      expect(failed.icon, Icons.error_outline);
      expect(
        recordingSessionStatus(session(phase: 'interrupted')).label,
        'Ghi bị gián đoạn',
      );

      final auto = recordingSessionStatus(session(reason: 'time_limit'));
      expect(auto.label, contains('hết thời gian'));
      final disk = recordingSessionStatus(session(reason: 'teacher_disk_low'));
      expect(disk.label, contains('ổ đĩa giảng viên'));
      expect({done.color, failed.color, auto.color}.length, 3);
    });

    test('the elapsed time of a recording Student is found and formatted', () {
      final tools = {
        'recordings': [
          {
            'targets': [
              {
                'id': 'a',
                'report': {'phase': 'recording', 'elapsed_seconds': 125},
              },
              {
                'id': 'b',
                'report': {'phase': 'stopped', 'elapsed_seconds': 9},
              },
            ],
          },
        ],
      };
      expect(recordingElapsedSeconds(tools, 'a'), 125);
      expect(recordingElapsedSeconds(tools, 'b'), isNull);
      expect(recordingElapsedSeconds(tools, 'zzz'), isNull);
      expect(recordingClock(125), '02:05');
      expect(recordingClock(3725), '1:02:05');
    });
  });
}
