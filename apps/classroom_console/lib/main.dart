import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'agent.dart';
import 'observe.dart';
import 'media_health.dart';
import 'diagnostics_dialog.dart';
import 'permissions_dialog.dart';
import 'observe_startup.dart';
import 'classroom_tools_ui.dart';
import 'settings.dart';
import 'file_transfer.dart';
import 'student_file_drop.dart';
import 'file_collection.dart';
import 'launch_apps.dart';
import 'open_website.dart';
import 'monitoring_badge.dart';
import 'hardware_control.dart';
import 'recording.dart';
import 'power_controls.dart';
import 'internet_access.dart';
import 'activity_window.dart';
import 'files_window.dart';
import 'vscode_access.dart';
import 'app_access.dart';
import 'remote_terminal.dart';
import 'app_dialog.dart';
import 'teacher_ribbon.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  final font = File(
    Platform.isMacOS
        ? '/System/Library/Fonts/SFNS.ttf'
        : '${Platform.environment['WINDIR'] ?? 'C:/Windows'}/Fonts/segoeui.ttf',
  );
  if (await font.exists()) {
    final loader = FontLoader('ClassroomSystem')
      ..addFont(font.readAsBytes().then((b) => ByteData.sublistView(b)));
    await loader.load();
  }
  await windowManager.ensureInitialized();
  final agent = Agent(
    List.of(args),
    closeApplication: () async {
      // Exit must terminate a hidden console as well as a visible window.
      // The agent releases media children; this closes each authenticated UI.
      try {
        await windowManager
            .setPreventClose(false)
            .timeout(const Duration(seconds: 1));
        await windowManager.destroy().timeout(const Duration(seconds: 1));
      } finally {
        exit(0);
      }
    },
  );
  final observeId = agent.option('--observe-id');
  final filesId = agent.option('--files-id');
  final activityId = agent.option('--activity-id');
  final internetIds = agent.option('--internet-ids');
  final vscodeIds = agent.option('--vscode-ids');
  final appsIds = agent.option('--apps-ids');
  final hardware = agent.option('--hardware-control') != null;
  final terminalId = agent.option('--terminal-id');
  const lifecycle = MethodChannel('classroom/lifecycle');
  lifecycle.setMethodCallHandler((call) async {
    if (call.method == 'reopenConsole' &&
        (observeId != null ||
            filesId != null ||
            activityId != null ||
            internetIds != null ||
            vscodeIds != null ||
            hardware ||
            appsIds != null ||
            terminalId != null)) {
      await agent.command('show_console');
    }
  });
  Rect? observeBounds;
  Uint8List? observePreview;
  if (observeId != null) {
    final previewPath = agent.option('--observe-preview');
    if (previewPath != null) {
      final previewFile = File(previewPath);
      try {
        if (await previewFile.exists() &&
            await previewFile.length() <= 4 * 1024 * 1024) {
          observePreview = await previewFile.readAsBytes();
        }
      } catch (_) {
        // The live Observe session still opens if the optional preview expired.
      } finally {
        try {
          if (await previewFile.exists()) await previewFile.delete();
        } catch (_) {}
      }
    }
    final x = double.tryParse(agent.option('--teacher-center-x') ?? '');
    final y = double.tryParse(agent.option('--teacher-center-y') ?? '');
    final center = x != null && y != null ? Offset(x, y) : null;
    try {
      observeBounds = observeWorkArea(
        await screenRetriever.getAllDisplays(),
        center,
      );
    } catch (_) {
      // Native maximize below is a usable fallback if monitor discovery fails.
    }
  }
  final theme = classroomTheme();
  final Widget home;
  final Size windowSize, minimumSize;
  final String title;
  if (hardware) {
    home = HardwareControlWindow(agent: agent);
    windowSize = const Size(1100, 760);
    minimumSize = const Size(820, 560);
    title = 'Hardware Control · Toàn bộ phòng máy';
  } else if (terminalId != null) {
    final name = agent.option('--files-name') ?? 'Student';
    home = RemoteTerminalWindow(
      agent: agent,
      studentId: terminalId,
      name: name,
    );
    windowSize = const Size(1000, 650);
    minimumSize = const Size(640, 400);
    title = 'Terminal từ xa · $name';
  } else if (filesId != null) {
    final name = agent.option('--files-name') ?? 'Student';
    home = FilesWindow(agent: agent, studentId: filesId, name: name);
    windowSize = const Size(1180, 720);
    minimumSize = const Size(820, 520);
    title = 'Tệp · $name';
  } else if (activityId != null) {
    final name = agent.option('--activity-name') ?? 'Student';
    home = ActivityWindow(
      agent: agent,
      studentId: activityId,
      studentName: name,
    );
    windowSize = const Size(1080, 760);
    minimumSize = const Size(760, 560);
    title = 'Hoạt động · $name';
  } else if (internetIds != null) {
    final audience = agent.option('--internet-audience') ?? 'cả lớp';
    home = InternetWindow(
      agent: agent,
      studentIds: internetIds.split(',').where((s) => s.isNotEmpty).toList(),
      audience: audience,
      classWide: agent.option('--internet-scope') != 'selected',
    );
    windowSize = const Size(980, 660);
    minimumSize = const Size(860, 560);
    title = 'Truy cập Internet · $audience';
  } else if (appsIds != null) {
    final audience = agent.option('--apps-audience') ?? 'cả lớp';
    home = AppsWindow(
      agent: agent,
      studentIds: appsIds.split(',').where((s) => s.isNotEmpty).toList(),
      audience: audience,
      classWide: agent.option('--apps-scope') != 'selected',
    );
    windowSize = const Size(900, 700);
    minimumSize = const Size(700, 520);
    title = 'Kiểm soát ứng dụng · $audience';
  } else if (vscodeIds != null) {
    final audience = agent.option('--vscode-audience') ?? 'cả lớp';
    home = VsCodeWindow(
      agent: agent,
      studentIds: vscodeIds.split(',').where((s) => s.isNotEmpty).toList(),
      audience: audience,
      classWide: agent.option('--vscode-scope') != 'selected',
    );
    windowSize = const Size(1020, 720);
    minimumSize = const Size(860, 560);
    title = 'Tiện ích VS Code · $audience';
  } else if (observeId != null) {
    final name = agent.option('--observe-name') ?? 'Student';
    final ip = agent.option('--observe-ip') ?? '';
    home = ObserveWindow(
      agent: agent,
      studentId: observeId,
      name: name,
      ip: ip,
      previewImage: observePreview,
    );
    windowSize = observeBounds?.size ?? const Size(1280, 800);
    minimumSize = Size(
      (observeBounds?.width ?? 800).clamp(0, 800),
      (observeBounds?.height ?? 600).clamp(0, 600),
    );
    title = 'Quan sát · $name${ip.isEmpty ? '' : ' · $ip'}';
  } else {
    home = ClassroomApp(agent: agent);
    windowSize = const Size(1280, 820);
    minimumSize = const Size(740, 520);
    title = 'Classroom · Giảng viên';
  }
  final dependent =
      hardware ||
      filesId != null ||
      activityId != null ||
      internetIds != null ||
      vscodeIds != null ||
      appsIds != null ||
      observeId != null ||
      terminalId != null;
  runApp(
    dependent
        ? MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: theme,
            home: home,
          )
        : home,
  );
  await WidgetsBinding.instance.waitUntilFirstFrameRasterized;
  await windowManager.waitUntilReadyToShow(
    WindowOptions(
      size: windowSize,
      minimumSize: minimumSize,
      title: title,
      backgroundColor: Color(0xfff3f5f9),
    ),
    () async {
      if (observeId != null) {
        if (observeBounds != null) {
          // Set the complete native frame before showing; no small-window flash
          // and no full-screen Space. Filling this rect also centers the window.
          await windowManager.setBounds(observeBounds);
        } else {
          await windowManager.maximize();
        }
      }
      await windowManager.show();
      await windowManager.focus();
    },
  );
  await agent.start();
}

/// Shared by the console and every child window so dialogs look the same.
ThemeData classroomTheme() => ThemeData(
  useMaterial3: true,
  fontFamily: 'ClassroomSystem',
  colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff156b79)),
  scaffoldBackgroundColor: const Color(0xfff3f5f9),
  visualDensity: VisualDensity.compact,
  cardTheme: const CardThemeData(elevation: 0, margin: EdgeInsets.zero),
  dialogTheme: DialogThemeData(
    backgroundColor: Colors.white,
    surfaceTintColor: Colors.transparent,
    elevation: 12,
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
    titleTextStyle: const TextStyle(
      fontFamily: 'ClassroomSystem',
      fontSize: 17,
      fontWeight: FontWeight.w700,
      color: Color(0xff1b2a36),
    ),
    contentTextStyle: const TextStyle(
      fontFamily: 'ClassroomSystem',
      fontSize: 13.5,
      height: 1.4,
      color: Color(0xff33434f),
    ),
    actionsPadding: const EdgeInsets.fromLTRB(24, 4, 24, 16),
  ),
  inputDecorationTheme: InputDecorationTheme(
    border: OutlineInputBorder(borderRadius: BorderRadius.circular(8)),
  ),
  filledButtonTheme: FilledButtonThemeData(
    style: FilledButton.styleFrom(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
    ),
  ),
  outlinedButtonTheme: OutlinedButtonThemeData(
    style: OutlinedButton.styleFrom(
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
    ),
  ),
);

class ClassroomApp extends StatelessWidget {
  const ClassroomApp({super.key, required this.agent});
  final Agent agent;
  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    title: 'Classroom',
    theme: classroomTheme(),
    home: Console(agent: agent),
  );
}

class Console extends StatefulWidget {
  const Console({super.key, required this.agent});
  final Agent agent;
  @override
  State<Console> createState() => _ConsoleState();
}

class _ConsoleState extends State<Console> with WindowListener {
  static const files = MethodChannel('classroom/files');
  String view = 'grid';
  String query = '';
  bool selectionMode = false;
  bool preparingFile = false;
  bool gridAuto = true;
  double gridSize = 300;
  final selected = <String>{};
  int request = 0;
  int ribbonTab = 0;
  Agent get agent => widget.agent;
  late final ToolActivity toolActivity = ToolActivity(agent);

  List<String> get toolRecipients {
    final ready = Set<String>.from(
      (agent.state['tools'] as Map? ?? const {})['ready'] as List? ?? const [],
    );
    final online = agent.students.where((r) => r['online'] == true);
    final targets = selectionMode
        ? online.where((r) => selected.contains(r['id']))
        : online;
    final ids = targets.map((r) => r['id'] as String).toList();
    if (ids.any((id) => !ready.contains(id))) return const [];
    return ids.where(ready.contains).toList();
  }

  Set<String> get lockedIds => {
    for (final id
        in ((agent.state['tools'] as Map? ?? const {})['locked'] as List? ??
            const []))
      '$id',
  };

  Set<String> get clipboardBlockedIds => {
    for (final id
        in ((agent.state['tools'] as Map? ?? const {})['clipboard_blocked']
                as List? ??
            const []))
      '$id',
  };

  bool get activityMonitoringOn =>
      (agent.state['tools'] as Map? ?? const {})['activity_monitoring'] == true;

  Future<void> toggleActivityMonitoring() => agent.request(
    'activity_monitoring_set',
    input: {'enabled': !activityMonitoringOn},
  );

  Future<void> internetAccess() async {
    final ids = toolRecipients;
    if (ids.isEmpty) return;
    // Yesterday's remembered rule selections are cleared before a window opens.
    await agent.expireDailyRules();
    final audience = selectionMode && selected.isNotEmpty
        ? '${ids.length} máy đã chọn'
        : 'cả lớp (${ids.length} máy online)';
    // Lets an already-open window come forward instead of silently doing
    // nothing when clicked again (mirrors Observe/Files/Activity).
    agent.pendingObserveFocus = 'internet';
    await agent.request(
      'internet_window',
      selected: ids,
      input: {
        'audience': audience,
        'scope': selectionMode && selected.isNotEmpty ? 'selected' : 'all',
      },
    );
  }

  Future<void> vscodeAccess() async {
    final ids = toolRecipients;
    if (ids.isEmpty) return;
    // Yesterday's remembered rule selections are cleared before a window opens.
    await agent.expireDailyRules();
    final audience = selectionMode && selected.isNotEmpty
        ? '${ids.length} máy đã chọn'
        : 'cả lớp (${ids.length} máy online)';
    agent.pendingObserveFocus = 'vscode';
    await agent.request(
      'vscode_window',
      selected: ids,
      input: {
        'audience': audience,
        'scope': selectionMode && selected.isNotEmpty ? 'selected' : 'all',
      },
    );
  }

  Future<void> appControl() async {
    final ids = toolRecipients;
    if (ids.isEmpty) return;
    // Yesterday's remembered rule selections are cleared before a window opens.
    await agent.expireDailyRules();
    final audience = selectionMode && selected.isNotEmpty
        ? '${ids.length} máy đã chọn'
        : 'cả lớp (${ids.length} máy online)';
    agent.pendingObserveFocus = 'apps';
    await agent.request(
      'app_window',
      selected: ids,
      input: {
        'audience': audience,
        'scope': selectionMode && selected.isNotEmpty ? 'selected' : 'all',
      },
    );
  }

  Future<void> powerAction(String op) {
    final ids = toolRecipients;
    return runPower(
      context,
      agent,
      op: op,
      ids: ids,
      audience: selectionMode && selected.isNotEmpty
          ? '${ids.length} máy đã chọn'
          : 'cả lớp (${ids.length} máy online)',
      classWide: !(selectionMode && selected.isNotEmpty),
    );
  }

  Future<void> launchApps() {
    final ids = toolRecipients;
    if (ids.isEmpty) return Future.value();
    return showLaunchDialog(
      context,
      agent,
      studentIds: ids,
      audience: selectionMode && selected.isNotEmpty
          ? '${ids.length} selected students'
          : 'cả lớp (${ids.length} máy online)',
    );
  }

  Future<void> openWebsite() {
    final ids = toolRecipients;
    if (ids.isEmpty) return Future.value();
    return showWebsiteDialog(
      context,
      agent,
      studentIds: ids,
      audience: selectionMode && selected.isNotEmpty
          ? '${ids.length} máy đã chọn'
          : 'cả lớp (${ids.length} máy online)',
    );
  }

  Future<void> collectFiles() {
    final ids = toolRecipients;
    if (ids.isEmpty) return Future.value();
    return showCollectDialog(
      context,
      agent,
      studentIds: ids,
      audience: selectionMode && selected.isNotEmpty
          ? '${ids.length} selected students'
          : 'cả lớp (${ids.length} máy online)',
    );
  }

  Future<void> collectMenu(Map<String, dynamic> row, Offset at) async {
    if (row['online'] != true) return;
    final studentId = '${row['id']}';
    final badgeEnabled = badgeState(agent, studentId)['enabled'] == true;
    final choice = await showMenu<String>(
      context: context,
      position: RelativeRect.fromLTRB(at.dx, at.dy, at.dx, at.dy),
      items: [
        PopupMenuItem(
          value: 'monitoring_badge',
          enabled: agent.connected && !agent.commandBusy,
          child: ListTile(
            dense: true,
            leading: Icon(badgeEnabled ? Icons.badge : Icons.badge_outlined),
            title: Text(badgeEnabled ? 'Tắt giám sát' : 'Bật giám sát'),
            subtitle: Text(badgeLabel(agent, studentId)),
          ),
        ),
        const PopupMenuItem(
          value: 'terminal',
          child: ListTile(
            dense: true,
            leading: Icon(Icons.terminal),
            title: Text('Terminal từ xa…'),
          ),
        ),
        const PopupMenuItem(
          value: 'files',
          child: ListTile(
            dense: true,
            leading: Icon(Icons.folder_open_outlined),
            title: Text('Thu tệp…'),
          ),
        ),
        const PopupMenuItem(
          value: 'observe',
          child: ListTile(
            dense: true,
            leading: Icon(Icons.monitor_outlined),
            title: Text('Quan sát màn hình'),
          ),
        ),
        PopupMenuItem(
          value: 'recording',
          enabled: agent.connected && !agent.commandBusy,
          child: const ListTile(
            dense: true,
            leading: Icon(Icons.video_camera_back_outlined),
            title: Text('Ghi màn hình'),
            subtitle: Text('Ghi máy này • Tự lưu MP4 về Desktop'),
          ),
        ),
        PopupMenuItem(
          value: 'screenshot',
          enabled: agent.connected && !agent.commandBusy,
          child: const ListTile(
            dense: true,
            leading: Icon(Icons.screenshot_monitor_outlined),
            title: Text('Chụp màn hình'),
            subtitle: Text('Lưu vào Desktop giảng viên'),
          ),
        ),
        PopupMenuItem(
          value: 'present',
          enabled:
              agent.connected &&
              !agent.commandBusy &&
              (agent.state['phase'] == 'idle' ||
                  (agent.state['phase'] == 'sharing' &&
                      agent.state['source_id'] != null)) &&
              agent.state['source_id'] != row['id'],
          child: const ListTile(
            dense: true,
            leading: Icon(Icons.present_to_all),
            title: Text(Vi.presentStudent),
          ),
        ),
      ],
    );
    if (!mounted) return;
    if (choice == 'monitoring_badge') {
      try {
        await agent.commandChecked(
          'monitoring_badge',
          [studentId],
          {'enabled': !badgeEnabled},
        );
      } catch (error) {
        if (!mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Không đổi được trạng thái giám sát: $error')),
        );
      }
    } else if (choice == 'terminal') {
      await agent.command('terminal_window', ['${row['id']}']);
    } else if (choice == 'files') {
      await agent.openFiles(row);
    } else if (choice == 'observe') {
      await inspect(row);
    } else if (choice == 'recording') {
      await showRecordings(context, agent, [studentId], startImmediately: true);
    } else if (choice == 'screenshot') {
      try {
        await screenshotStudent(row);
      } catch (error) {
        if (!mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Không chụp được màn hình: $error')),
        );
      }
    } else if (choice == 'present') {
      // The context target is independent of checkbox selection. The backend
      // revalidates online/current/busy state before stopping any old source.
      await presentStudent([row['id'] as String]);
    }
  }

  final screenshotBusy = <String>{};

  Future<void> screenshotStudent(Map<String, dynamic> row) async {
    final id = '${row['id']}';
    if (!screenshotBusy.add(id)) return;
    Directory? staging;
    String? jobId;
    try {
      staging = await Directory(
        agent.endpoint.parent.path,
      ).createTemp('screenshot-');
      final response = await agent.request(
        'student_screenshot',
        selected: [id],
        input: {'dest': staging.path},
      );
      if (response['ok'] != true) throw StateError(Agent.failure(response));
      jobId = response['job_id'] as String?;
      if (jobId == null) throw StateError('Screenshot request was not queued');
      final deadline = DateTime.now().add(const Duration(seconds: 60));
      while (true) {
        if (!mounted) throw StateError('Teacher window closed');
        if (!agent.connected) throw StateError('Teacher connection lost');
        if (DateTime.now().isAfter(deadline)) {
          throw StateError('Student screenshot timed out');
        }
        final jobs = agent.fileJobs.where((j) => j['id'] == jobId);
        final targets = jobs.isEmpty
            ? const <Map>[]
            : (jobs.first['targets'] as List).whereType<Map>().toList();
        final target = targets.where((t) => t['id'] == id).firstOrNull;
        if (target != null) {
          if (['error', 'cancelled'].contains(target['status'])) {
            throw StateError('${target['error'] ?? 'Screenshot cancelled'}');
          }
          if (target['status'] == 'complete') break;
        }
        await Future<void>.delayed(const Duration(milliseconds: 200));
      }
      final image = File('${staging.path}/Screenshot.png');
      final length = await image.length();
      if (length < 8 || length > 32 * 1024 * 1024) {
        throw StateError('Invalid screenshot file size');
      }
      final path = await files.invokeMethod<String>('saveScreenshot', {
        'name': '${row['name']}',
        'png': await image.readAsBytes(),
      });
      if (mounted && path != null) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              'Đã lưu ảnh chụp vào Desktop · ${path.split(RegExp(r'[/\\]')).last}',
            ),
          ),
        );
      }
      jobId = null;
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              error is PlatformException
                  ? error.message ?? 'Không lưu được ảnh chụp'
                  : '$error',
            ),
          ),
        );
      }
    } finally {
      if (jobId != null) await agent.request('fs_cancel', input: {'id': jobId});
      if (staging != null && await staging.exists()) {
        await staging.delete(recursive: true);
      }
      screenshotBusy.remove(id);
    }
  }

  Future<void> presentStudent(Iterable<String> ids) async {
    try {
      final selected = ids.toList();
      String? previewPath;
      if (selected.length == 1) {
        try {
          final row = agent.students.firstWhere(
            (student) => student['id'] == selected.single,
          );
          previewPath = await agent.stageObservePreview(row);
        } catch (error) {
          await agent.log('Student broadcast preview staging failed: $error');
        }
      }
      await agent.commandChecked('switch_source', selected, {
        if (previewPath != null) 'preview_path': previewPath,
      });
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Không chiếu được màn hình học viên: $error')),
      );
    }
  }

  Future<void> sendClassMessage() async {
    final controller = TextEditingController();
    final targetCount = toolRecipients.length;
    final target = selectionMode && selected.isNotEmpty
        ? '$targetCount máy đã chọn'
        : 'cả lớp ($targetCount máy online)';
    final text = await showDialog<String>(
      context: context,
      builder: (context) {
        return StatefulBuilder(
          builder: (context, setLocal) => AppDialog(
            icon: Icons.chat_outlined,
            title: 'Gửi tin nhắn',
            subtitle: 'Tin nhắn hiện trên màn hình của $target.',
            width: 520,
            footerLeading: Text(
              '${controller.text.characters.length}/2000',
              style: const TextStyle(fontSize: 12, color: AppDialog.muted),
            ),
            actions: [
              AppDialog.cancel(context),
              AppDialog.primary(
                label: 'Gửi',
                icon: Icons.send_outlined,
                onPressed: controller.text.trim().isEmpty
                    ? null
                    : () => Navigator.pop(context, controller.text),
              ),
            ],
            child: TextField(
              controller: controller,
              autofocus: true,
              minLines: 5,
              maxLines: 8,
              maxLength: 2000,
              buildCounter:
                  (
                    context, {
                    required currentLength,
                    required isFocused,
                    maxLength,
                  }) => null,
              onChanged: (_) => setLocal(() {}),
              decoration: const InputDecoration(
                hintText: 'Nhập nội dung gửi tới học viên…',
              ),
            ),
          ),
        );
      },
    );
    controller.dispose();
    if (text == null || text.trim().isEmpty) return;
    final recipients = toolRecipients;
    if (recipients.isEmpty) return;
    await agent.command('tool_message', recipients, {
      'text': text.trim(),
      'allow_reply': true,
    });
  }

  Future<void> sendClassFile() async {
    final directory = agent.dataDirectory;
    if (directory == null) return;
    setState(() => preparingFile = true);
    try {
      final picked = await files.invokeMapMethod<String, dynamic>(
        'chooseFile',
        {'directory': agent.endpoint.parent.path},
      );
      if (picked == null || !mounted) return;
      final recipients = toolRecipients;
      if (recipients.isEmpty) return;
      await agent.command('tool_file', recipients, picked);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              e is PlatformException
                  ? e.message ?? 'Không chuẩn bị được tệp'
                  : 'Không chuẩn bị được tệp: $e',
            ),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => preparingFile = false);
    }
  }

  @override
  void initState() {
    super.initState();
    // Baseline existing agent jobs now so old results are never re-announced.
    toolActivity;
    wasSharing = agent.state['phase'] == 'sharing';
    havePresentationState = agent.state.containsKey('phase');
    agent.addListener(changed);
    toolActivity.addListener(refresh);
    windowManager.addListener(this);
    windowManager.setPreventClose(true);
    if (Platform.isMacOS) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) maybeShowMacPermissions(context, agent);
      });
    }
  }

  bool loadedView = false;
  bool wasSharing = false;
  bool havePresentationState = false;

  /// macOS only: ask the OS for Screen Recording when sharing is attempted
  /// without it. Other platforms never prompt.
  Future<bool> ensureScreenAccess() async {
    if (!Platform.isMacOS) return true;
    try {
      final granted = await permissions.invokeMethod<bool>(
        'screenRecordingGranted',
      );
      if (granted == false) {
        await permissions.invokeMethod<void>('openScreenRecordingSettings');
        return false;
      }
    } catch (_) {
      /* Agent capture errors remain authoritative. */
    }
    return true;
  }

  Future<void> shareClass() async {
    if (await ensureScreenAccess()) await agent.command('share_class');
  }

  Future<void> shareSelected(List<String> ids) async {
    if (await ensureScreenAccess()) {
      await agent.command('share_selected', ids);
    }
  }

  static const permissions = MethodChannel('classroom/permissions');
  void refresh() {
    if (mounted) setState(() {});
  }

  void changed() {
    if (!loadedView && agent.connected) {
      view =
          agent.preferences['view'] as String? ??
          (agent.preferences['grid'] == false ? 'list' : 'grid');
      gridAuto = agent.preferences['grid_auto'] as bool? ?? true;
      gridSize = (agent.preferences['grid_size'] as num? ?? 300)
          .toDouble()
          .clamp(180, 480);
      loadedView = true;
    }
    final n = agent.state['ui_request'] as int? ?? 0;
    final sharing = agent.state['phase'] == 'sharing';
    // Minimize only on confirmed presentation start, never while capture is
    // starting or when reopening a console for an already active stream.
    if (havePresentationState &&
        sharing &&
        !wasSharing &&
        n <= request &&
        agent.state['source_id'] == null) {
      windowManager.minimize();
    }
    wasSharing = sharing;
    havePresentationState = agent.state.containsKey('phase');
    if (n > request) {
      request = n;
      windowManager.restore().then((_) async {
        await windowManager.show();
        await windowManager.focus();
      });
      if (agent.state['open_settings'] == true) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) {
            settings();
          }
        });
      }
    }
    if (mounted) {
      setState(() {});
    }
  }

  @override
  void onWindowClose() {
    windowManager.hide();
  }

  @override
  void dispose() {
    agent.removeListener(changed);
    toolActivity.removeListener(refresh);
    toolActivity.dispose();
    windowManager.removeListener(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final rows = agent.students
        .where(
          (r) => '${r['name']} ${r['ip']}'.toLowerCase().contains(
            query.toLowerCase(),
          ),
        )
        .toList();
    final online = agent.students.where((r) => r['online'] == true).length;
    final capturePermissionError =
        agent.error?.contains('TCC') == true ||
        agent.error?.contains('declined') == true;
    // Missing Screen Recording is handled when the teacher presses Share: the
    // system dialog appears then, so no standing warning is shown here.
    final shownError = capturePermissionError ? null : agent.error;
    final phase = agent.state['phase'] ?? 'connecting';
    final idle = phase == 'idle' && agent.connected && !agent.commandBusy;
    final onlineSelected = agent.students
        .where((r) => r['online'] == true && selected.contains(r['id']))
        .map((r) => r['id'] as String)
        .toSet();
    final canSwitch =
        agent.connected &&
        !agent.commandBusy &&
        (phase == 'idle' || phase == 'sharing');
    final sourceId = agent.state['source_id'];
    var sourceName = 'Giảng viên';
    for (final row in agent.students) {
      if (row['id'] == sourceId) {
        sourceName = row['name'] as String? ?? 'Student';
      }
    }
    final canUseTools = toolRecipients.isNotEmpty;
    return Scaffold(
      body: Stack(
        children: [
          Column(
            children: [
              Container(
                height: 38,
                color: const Color(0xff142735),
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: Row(
                  children: [
                    const Icon(
                      Icons.school_outlined,
                      color: Colors.white,
                      size: 18,
                    ),
                    const SizedBox(width: 8),
                    const Text(
                      'Classroom',
                      style: TextStyle(
                        color: Colors.white,
                        fontSize: 14,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    if (MediaQuery.sizeOf(context).width >= 900)
                      const SizedBox(width: 10),
                    if (MediaQuery.sizeOf(context).width >= 900)
                      const Text(
                        'Giảng viên',
                        style: TextStyle(
                          color: Color(0xffaabdc9),
                          fontSize: 11,
                        ),
                      ),
                    const Spacer(),
                    Text(
                      MediaQuery.sizeOf(context).width < 900
                          ? '$online / ${agent.students.length} online'
                          : '$online online / ${agent.students.length} máy',
                      style: const TextStyle(
                        color: Color(0xffc2d0d9),
                        fontSize: 12,
                      ),
                    ),
                    if (!agent.connected ||
                        phase != 'idle' ||
                        agent.commandBusy)
                      Padding(
                        padding: const EdgeInsets.only(left: 14),
                        child: Text(
                          !agent.connected
                              ? 'Đang kết nối…'
                              : agent.commandBusy
                              ? 'Đang xử lý…'
                              : phase == 'sharing'
                              ? 'Đang chiếu màn hình'
                              : phase == 'switching'
                              ? 'Đang đổi nguồn chiếu…'
                              : phase == 'starting'
                              ? 'Đang bắt đầu…'
                              : 'Đang dừng…',
                          style: const TextStyle(
                            color: Color(0xff87d4dc),
                            fontSize: 12,
                          ),
                        ),
                      ),
                    const SizedBox(width: 10),
                    if (Platform.isMacOS)
                      IconButton(
                        tooltip: 'Quyền macOS',
                        onPressed: () => showMacPermissions(context, agent),
                        icon: const Icon(
                          Icons.verified_user_outlined,
                          size: 17,
                          color: Colors.white70,
                        ),
                      ),
                    SizedBox(
                      width: 28,
                      height: 28,
                      child: IconButton(
                        padding: EdgeInsets.zero,
                        tooltip: 'Cài đặt lớp học',
                        onPressed: idle ? settings : null,
                        icon: Icon(
                          Icons.settings_outlined,
                          size: 17,
                          color: idle ? Colors.white70 : Colors.white24,
                        ),
                      ),
                    ),
                    const SizedBox(width: 6),
                    SizedBox(
                      width: 28,
                      height: 28,
                      child: IconButton(
                        padding: EdgeInsets.zero,
                        tooltip: 'Thoát Classroom',
                        onPressed: () async {
                          await agent.command('quit');
                          await windowManager.setPreventClose(false);
                          await windowManager.destroy();
                        },
                        icon: const Icon(
                          Icons.power_settings_new,
                          color: Colors.white70,
                          size: 17,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              Container(
                width: double.infinity,
                decoration: const BoxDecoration(
                  color: Colors.white,
                  border: Border(bottom: BorderSide(color: Color(0xffdfe5eb))),
                ),
                padding: const EdgeInsets.symmetric(
                  horizontal: 16,
                  vertical: 7,
                ),
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final canPresentSelected =
                        agent.connected &&
                        (phase == 'idle' ||
                            (phase == 'sharing' && sourceId != null)) &&
                        selected.length == 1;
                    final bigIcon = const BoxConstraints(
                      minWidth: 40,
                      minHeight: 36,
                    );
                    final actions = Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (agent.connected && phase == 'idle')
                          IconButton.filled(
                            tooltip: Vi.shareClass,
                            constraints: bigIcon,
                            onPressed: idle && online > 0 ? shareClass : null,
                            icon: const Icon(Icons.cast, size: 22),
                          ),
                        if (agent.connected &&
                            (phase == 'sharing' ||
                                phase == 'starting' ||
                                phase == 'switching'))
                          IconButton.filled(
                            tooltip: Vi.stopSharing,
                            constraints: bigIcon,
                            style: IconButton.styleFrom(
                              backgroundColor: const Color(0xffb83b40),
                              foregroundColor: Colors.white,
                            ),
                            onPressed: agent.commandBusy
                                ? null
                                : () => agent.command('stop'),
                            icon: const Icon(Icons.stop, size: 22),
                          ),
                        if (phase == 'sharing')
                          Padding(
                            padding: const EdgeInsets.only(left: 8),
                            child: ConstrainedBox(
                              constraints: const BoxConstraints(maxWidth: 180),
                              child: Text(
                                '${Vi.presenting}: $sourceName',
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(
                                  fontSize: 12,
                                  fontWeight: FontWeight.w600,
                                  color: Color(0xff1f7a4d),
                                ),
                              ),
                            ),
                          ),
                        if (agent.connected && phase == 'stopping')
                          const Text(
                            'Đang kết thúc phiên chiếu…',
                            style: TextStyle(
                              fontSize: 12,
                              color: Colors.blueGrey,
                            ),
                          ),
                        if (phase == 'sharing' && sourceId != null)
                          IconButton.outlined(
                            tooltip: Vi.backToTeacher,
                            constraints: bigIcon,
                            onPressed: canSwitch
                                ? () => agent.command('switch_source')
                                : null,
                            icon: const Icon(Icons.screen_share_outlined),
                          ),
                        if (agent.connected &&
                            phase == 'idle' &&
                            selected.isNotEmpty)
                          IconButton.outlined(
                            tooltip:
                                '${Vi.shareSelected} (${onlineSelected.length})',
                            constraints: bigIcon,
                            onPressed: idle && onlineSelected.isNotEmpty
                                ? () => shareSelected(onlineSelected.toList())
                                : null,
                            icon: Badge(
                              label: Text('${onlineSelected.length}'),
                              child: const Icon(Icons.screen_share_outlined),
                            ),
                          ),
                        if (canPresentSelected)
                          IconButton.outlined(
                            tooltip: onlineSelected.isEmpty
                                ? 'Máy học viên đã chọn đang offline'
                                : Vi.presentStudent,
                            constraints: bigIcon,
                            onPressed:
                                canSwitch &&
                                    onlineSelected.length == 1 &&
                                    onlineSelected.single != sourceId
                                ? () => presentStudent(onlineSelected)
                                : null,
                            icon: const Icon(Icons.present_to_all),
                          ),
                      ].expand((w) => [w, const SizedBox(width: 6)]).toList(),
                    );
                    final toolsEnabled = !agent.commandBusy && canUseTools;
                    final collectActive = agent.fileJobs.any(
                      (j) =>
                          j['kind'] == 'collect' &&
                          (j['targets'] as List).any(
                            (t) => jobTargetActive(
                              Map<String, dynamic>.from(t as Map),
                            ),
                          ),
                    );
                    final recordIds = selectionMode && selected.isNotEmpty
                        ? onlineSelected.toList()
                        : agent.students
                              .where((r) => r['online'] == true)
                              .map((r) => '${r['id']}')
                              .toList();
                    final badgeIds = selectionMode && selected.isNotEmpty
                        ? selected.toList()
                        : agent.students.map((r) => '${r['id']}').toList();
                    final ribbonTabs = <RibbonTab>[
                      RibbonTab('Kiểm soát lớp', Icons.shield_outlined, [
                        RibbonItem(
                          icon: Icons.language,
                          label: 'Mở website',
                          tooltip: 'Mở một trang web trên máy học viên',
                          onPressed: toolsEnabled ? openWebsite : null,
                        ),
                        RibbonItem(
                          icon: Icons.rocket_launch_outlined,
                          label: 'Chạy ứng dụng',
                          tooltip: 'Khởi chạy ứng dụng trên máy học viên',
                          onPressed: toolsEnabled ? launchApps : null,
                        ),
                        RibbonItem(
                          icon: Icons.public_off_outlined,
                          label: 'Internet',
                          tooltip: 'Giới hạn truy cập Internet',
                          badge: restrictedStudents(agent).length,
                          onPressed: toolsEnabled ? internetAccess : null,
                        ),
                        RibbonItem(
                          icon: Icons.block_outlined,
                          label: 'Chặn ứng dụng',
                          tooltip: 'Kiểm soát ứng dụng được phép chạy',
                          badge: appsBlockedStudents(agent).length,
                          onPressed: toolsEnabled ? appControl : null,
                        ),
                        RibbonItem(
                          icon: Icons.extension_outlined,
                          label: 'VS Code',
                          tooltip: 'Kiểm soát tiện ích mở rộng của VS Code',
                          badge: vscodeBlockedStudents(agent).length,
                          onPressed: toolsEnabled ? vscodeAccess : null,
                        ),
                        RibbonItem(
                          icon: Icons.power_settings_new,
                          label: 'Khóa / Tắt máy',
                          tooltip:
                              'Khóa màn hình, đăng xuất, khởi động lại hoặc tắt máy',
                          badge: lockedIds.length + clipboardBlockedIds.length,
                          menu: () => powerMenuItems(
                            anyLocked: lockedIds.isNotEmpty,
                            anyClipboardBlocked: clipboardBlockedIds.isNotEmpty,
                          ),
                          onMenu: toolsEnabled ? powerAction : null,
                        ),
                      ]),
                      RibbonTab('Tin nhắn & tệp', Icons.folder_shared_outlined, [
                        RibbonItem(
                          icon: Icons.download_for_offline_outlined,
                          label: 'Thu bài / tệp',
                          tooltip: 'Thu tệp từ máy học viên',
                          dot: collectActive,
                          menu: () => [
                            const PopupMenuItem(
                              value: 'new',
                              child: Text('Thu tệp…'),
                            ),
                            PopupMenuItem(
                              value: 'progress',
                              enabled: agent.fileJobs.any(
                                (j) => j['kind'] == 'collect',
                              ),
                              child: const Text('Tiến độ thu tệp'),
                            ),
                          ],
                          onMenu: toolsEnabled
                              ? (action) {
                                  if (action == 'new') {
                                    collectFiles();
                                  } else {
                                    showCollectProgress(context, agent);
                                  }
                                }
                              : null,
                        ),
                        RibbonItem(
                          icon: Icons.attach_file,
                          label: 'Gửi tệp',
                          tooltip:
                              'Gửi tệp tới học viên (hoặc kéo thả lên thẻ máy)',
                          onPressed: toolsEnabled ? sendClassFile : null,
                        ),
                        RibbonItem(
                          icon: Icons.chat_outlined,
                          label: 'Gửi tin nhắn',
                          tooltip: 'Gửi tin nhắn tới học viên',
                          onPressed: toolsEnabled ? sendClassMessage : null,
                        ),
                        RibbonItem(
                          icon: Icons.forum_outlined,
                          label: 'Phản hồi',
                          tooltip: toolActivity.unread == 0
                              ? 'Phản hồi từ học viên'
                              : '${toolActivity.unread} phản hồi mới',
                          badge: toolActivity.unread,
                          onPressed: () =>
                              showRepliesDialog(context, toolActivity),
                        ),
                      ]),
                      RibbonTab('Giám sát', Icons.visibility_outlined, [
                        RibbonItem(
                          icon: Icons.visibility_outlined,
                          label: 'Theo dõi hoạt động',
                          tooltip: activityMonitoringOn
                              ? 'Đang bật: học viên thu thập cửa sổ, tab trình duyệt và tệp tải về. Bấm để tắt.'
                              : 'Đang tắt. Bấm để bật theo dõi hoạt động cho cả lớp.',
                          active: activityMonitoringOn,
                          onPressed: !agent.commandBusy
                              ? toggleActivityMonitoring
                              : null,
                        ),
                        RibbonItem(
                          icon: Icons.badge_outlined,
                          label: 'Huy hiệu giám sát',
                          tooltip:
                              'Bật/tắt huy hiệu "đang được giám sát" trên máy học viên',
                          onPressed:
                              agent.connected &&
                                  !agent.commandBusy &&
                                  agent.students.isNotEmpty
                              ? () => showMonitoringBadge(
                                  context,
                                  agent,
                                  badgeIds,
                                )
                              : null,
                        ),
                        RibbonItem(
                          icon: Icons.video_camera_back_outlined,
                          label: 'Ghi màn hình',
                          tooltip:
                              'Ghi màn hình các máy đã chọn hoặc toàn bộ máy online',
                          onPressed: agent.connected
                              ? () => showRecordings(context, agent, recordIds)
                              : null,
                        ),
                        RibbonItem(
                          icon: Icons.link_off,
                          label: 'Mất kết nối',
                          tooltip: toolActivity.disconnects.isEmpty
                              ? 'Cảnh báo mất kết nối'
                              : '${toolActivity.disconnects.length} cảnh báo mất kết nối',
                          badge: toolActivity.unreadDisconnects,
                          badgeColor: const Color(0xffb83b40),
                          onPressed: () =>
                              showDisconnectsDialog(context, toolActivity),
                        ),
                      ]),
                      RibbonTab('Hệ thống', Icons.tune, [
                        RibbonItem(
                          icon: Icons.usb_outlined,
                          label: 'Phần cứng',
                          tooltip:
                              'Điều khiển phần cứng (USB, Wi-Fi, Bluetooth…) cả phòng máy',
                          onPressed: agent.connected && !agent.commandBusy
                              ? () {
                                  agent.pendingObserveFocus = 'hardware';
                                  agent.command('hardware_window');
                                }
                              : null,
                        ),
                        RibbonItem(
                          icon: Icons.bug_report_outlined,
                          label: 'Chẩn đoán',
                          tooltip: selected.isEmpty
                              ? 'Thu thập chẩn đoán cả lớp'
                              : 'Thu thập chẩn đoán các máy đã chọn',
                          onPressed: agent.connected
                              ? () => collectDiagnostics(
                                  context,
                                  agent,
                                  selected.toList(),
                                )
                              : null,
                        ),
                      ]),
                    ];
                    final rosterTools = Wrap(
                      spacing: 8,
                      runSpacing: 6,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        SizedBox(
                          width: constraints.maxWidth < 900 ? 170 : 220,
                          height: 32,
                          child: TextField(
                            onChanged: (v) => setState(() => query = v),
                            style: const TextStyle(fontSize: 12),
                            decoration: InputDecoration(
                              hintText: Vi.search,
                              isDense: true,
                              contentPadding: const EdgeInsets.symmetric(
                                horizontal: 10,
                                vertical: 8,
                              ),
                              prefixIcon: const Icon(Icons.search, size: 17),
                              prefixIconConstraints: const BoxConstraints(
                                minWidth: 32,
                              ),
                              filled: true,
                              fillColor: const Color(0xfff3f5f9),
                              border: OutlineInputBorder(
                                borderRadius: BorderRadius.circular(6),
                                borderSide: BorderSide.none,
                              ),
                            ),
                          ),
                        ),
                        IconButton(
                          visualDensity: VisualDensity.compact,
                          tooltip: Vi.rescan,
                          onPressed: agent.connected && !agent.commandBusy
                              ? agent.rescan
                              : null,
                          icon: const Icon(Icons.refresh, size: 18),
                        ),
                        Tooltip(
                          message: selectionMode
                              ? Vi.donePicking
                              : Vi.pickMachinesTip,
                          child: selectionMode
                              ? FilledButton.tonalIcon(
                                  style: toolbarButton(),
                                  onPressed: () => setState(() {
                                    selectionMode = false;
                                    selected.clear();
                                  }),
                                  icon: const Icon(Icons.check, size: 16),
                                  label: Text(
                                    selected.isEmpty
                                        ? Vi.donePicking
                                        : '${Vi.donePicking} (${selected.length})',
                                  ),
                                )
                              : OutlinedButton.icon(
                                  style: toolbarButton(),
                                  onPressed: () =>
                                      setState(() => selectionMode = true),
                                  icon: const Icon(Icons.checklist, size: 16),
                                  label: const Text(Vi.pickMachines),
                                ),
                        ),
                        if (selectionMode) ...[
                          TextButton(
                            onPressed: rows.any((r) => r['online'] == true)
                                ? () => setState(
                                    () => selected.addAll(
                                      rows
                                          .where((r) => r['online'] == true)
                                          .map((r) => r['id'] as String),
                                    ),
                                  )
                                : null,
                            child: const Text(Vi.selectAllOnline),
                          ),
                          TextButton(
                            onPressed: selected.isNotEmpty
                                ? () => setState(selected.clear)
                                : null,
                            child: const Text(Vi.clearSelection),
                          ),
                        ],
                        ToggleButtons(
                          borderRadius: BorderRadius.circular(6),
                          constraints: const BoxConstraints(
                            minWidth: 32,
                            minHeight: 30,
                          ),
                          isSelected: [
                            'grid',
                            'list',
                            'compact',
                          ].map((v) => v == view).toList(),
                          onPressed: (i) {
                            setState(
                              () => view = ['grid', 'list', 'compact'][i],
                            );
                            agent.savePreferences({'view': view});
                          },
                          children: const [
                            Tooltip(
                              message: 'Dạng lưới',
                              child: Icon(Icons.grid_view, size: 16),
                            ),
                            Tooltip(
                              message: 'Dạng danh sách',
                              child: Icon(Icons.view_list_outlined, size: 17),
                            ),
                            Tooltip(
                              message: 'Danh sách gọn',
                              child: Icon(Icons.format_list_bulleted, size: 17),
                            ),
                          ],
                        ),
                      ],
                    );
                    if (!agent.connected) return rosterTools;
                    return TeacherRibbon(
                      tabs: ribbonTabs,
                      index: ribbonTab,
                      onTab: (i) => setState(() => ribbonTab = i),
                      leading: actions,
                      trailing: rosterTools,
                    );
                  },
                ),
              ),
              if (shownError != null)
                MaterialBanner(
                  content: Text(shownError),
                  actions: [
                    TextButton(
                      onPressed: () {
                        if (agent.connected) {
                          agent.command('clear_error');
                        } else {
                          agent.start();
                        }
                      },
                      child: Text(agent.connected ? 'Bỏ qua' : 'Kết nối lại'),
                    ),
                  ],
                ),
              const SizedBox(height: 12),
              Expanded(
                child: rows.isEmpty
                    ? Center(
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Icon(
                              Icons.devices_outlined,
                              size: 64,
                              color: Colors.blueGrey.shade300,
                            ),
                            const SizedBox(height: 16),
                            Text(
                              agent.connected
                                  ? (query.isEmpty ? Vi.noStudents : Vi.noMatch)
                                  : Vi.connecting,
                              style: const TextStyle(
                                fontSize: 21,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(height: 8),
                            const Text(Vi.waiting),
                          ],
                        ),
                      )
                    : Padding(
                        padding: const EdgeInsets.fromLTRB(24, 0, 24, 20),
                        child: LayoutBuilder(
                          builder: (context, c) => view == 'grid'
                              ? GridView.builder(
                                  gridDelegate: gridLayout(c, rows.length),
                                  itemCount: rows.length,
                                  itemBuilder: (context, i) =>
                                      fileDrop(rows[i], card(rows[i])),
                                )
                              : ListView.separated(
                                  itemCount: rows.length,
                                  separatorBuilder: (_, i) =>
                                      const SizedBox(height: 8),
                                  itemBuilder: (context, i) => fileDrop(
                                    rows[i],
                                    view == 'compact'
                                        ? compact(rows[i])
                                        : card(rows[i], list: true),
                                  ),
                                ),
                        ),
                      ),
              ),
              Container(
                color: Colors.white,
                padding: const EdgeInsets.symmetric(
                  horizontal: 24,
                  vertical: 2,
                ),
                child: Row(
                  children: [
                    Icon(
                      agent.connected ? Icons.link : Icons.link_off,
                      size: 16,
                      color: Colors.blueGrey,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        [
                          agent.connected
                              ? 'Giảng viên ${agent.state['teacher_ip']}'
                              : 'Mất kết nối với dịch vụ nền',
                          ?agent.notice,
                        ].join(' · '),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(fontSize: 12),
                      ),
                    ),
                    if (view == 'grid') ...[
                      const Icon(
                        Icons.photo_size_select_large,
                        size: 15,
                        color: Colors.blueGrey,
                      ),
                      SizedBox(
                        width: MediaQuery.sizeOf(context).width < 900
                            ? 90
                            : 160,
                        height: 26,
                        child: Slider(
                          value: gridSize,
                          min: 180,
                          max: 480,
                          divisions: 15,
                          label: '${gridSize.round()} px',
                          semanticFormatterCallback: (v) =>
                              'Kích thước ô ${v.round()} pixel',
                          onChanged: (value) => setState(() {
                            gridSize = value;
                            gridAuto = false;
                          }),
                          onChangeEnd: (_) => agent.savePreferences({
                            'grid_auto': gridAuto,
                            'grid_size': gridSize,
                          }),
                        ),
                      ),
                      FilterChip(
                        label: const Text(
                          'Tự động',
                          style: TextStyle(fontSize: 11),
                        ),
                        selected: gridAuto,
                        visualDensity: VisualDensity.compact,
                        materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                        onSelected: (value) {
                          setState(() => gridAuto = value);
                          agent.savePreferences({
                            'grid_auto': gridAuto,
                            'grid_size': gridSize,
                          });
                        },
                      ),
                    ],
                    const SizedBox(width: 16),
                    Icon(
                      Icons.computer_outlined,
                      size: 15,
                      color: online > 0
                          ? const Color(0xff1f7a4d)
                          : Colors.blueGrey,
                    ),
                    const SizedBox(width: 6),
                    Text(
                      '$online/${agent.students.length} kết nối',
                      style: TextStyle(
                        fontSize: 11.5,
                        fontWeight: FontWeight.w600,
                        color: online > 0
                            ? const Color(0xff1f7a4d)
                            : Colors.blueGrey,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
          Positioned(
            right: 20,
            bottom: 36,
            child: ClassroomToolsOverlay(
              agent: agent,
              activity: toolActivity,
              preparingFile: preparingFile,
              onOpenReplies: () => showRepliesDialog(context, toolActivity),
            ),
          ),
        ],
      ),
    );
  }

  SliverGridDelegateWithFixedCrossAxisCount gridLayout(
    BoxConstraints c,
    int count,
  ) {
    final columns = gridColumns(c.maxWidth, count, c.maxHeight);
    final itemWidth = (c.maxWidth - (columns - 1) * 16) / columns;
    agent.thumbnailSize(itemWidth);
    return SliverGridDelegateWithFixedCrossAxisCount(
      crossAxisCount: columns,
      crossAxisSpacing: 16,
      mainAxisSpacing: 16,
      mainAxisExtent: itemWidth * 9 / 16,
    );
  }

  int gridColumns(double width, int count, double height) {
    if (gridAuto) {
      var columns = ((width + 16) / 316).round().clamp(1, count.clamp(1, 12));
      // Avoid an oversized single card hiding its title/actions below the viewport.
      final fittingWidth = (height - 8).clamp(100, double.infinity) * 16 / 9;
      final fittingColumns = ((width + 16) / (fittingWidth + 16)).ceil().clamp(
        1,
        12,
      );
      if (columns < fittingColumns) columns = fittingColumns;
      return columns;
    }
    return ((width + 16) / (gridSize + 16)).floor().clamp(1, 12);
  }

  ButtonStyle toolbarButton() => FilledButton.styleFrom(
    minimumSize: const Size(0, 32),
    padding: const EdgeInsets.symmetric(horizontal: 12),
    textStyle: const TextStyle(fontSize: 12, fontWeight: FontWeight.w500),
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
  );

  Future<void> settings() async {
    final result = await showDialog<TeacherSettings>(
      context: context,
      builder: (_) =>
          TeacherSettingsDialog(agent: agent, selected: selected.toList()),
    );
    if (result != null) {
      await agent.configure(result.ip, result.port, result.overlay);
    }
  }

  String status(Map<String, dynamic> r) {
    if (r['online'] != true) {
      return agent.seenOnlineIds.contains(r['id'])
          ? 'Mất kết nối'
          : 'Chưa kết nối trong phiên này';
    }
    final health = mediaHealthLabel(r);
    if (health.isNotEmpty) return health;
    if (r['monitoring'] == 'observing') {
      return 'Đang quan sát trực tiếp';
    }
    if (r['monitoring'] == 'paused') {
      return 'Đang xem bài chiếu';
    }
    if (r['monitoring'] == 'unavailable') {
      return 'Chưa có ảnh màn hình';
    }
    if ((r['thumbnail_age_ms'] as num? ?? 0) > 10000) {
      return 'Ảnh màn hình đã cũ';
    }
    return 'Online';
  }

  Widget connectionDot(Map<String, dynamic> r) {
    final dot = _statusDot(r);
    final internet = restrictedStudents(agent)['${r['id']}'];
    final locked = lockedIds.contains(r['id']);
    final clipboardBlocked = clipboardBlockedIds.contains(r['id']);
    final badge = badgeState(agent, '${r['id']}');
    if (!locked &&
        !clipboardBlocked &&
        internet == null &&
        badge['enabled'] != true &&
        badge['status'] != 'error') {
      return dot;
    }
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (badge['enabled'] == true || badge['status'] == 'error')
          Tooltip(
            message: badgeLabel(agent, '${r['id']}'),
            child: Icon(
              Icons.visibility,
              size: 15,
              color: badge['status'] == 'applied'
                  ? const Color(0xffc91f28)
                  : const Color(0xffdb8a22),
            ),
          ),
        if (internet != null)
          Tooltip(
            message: 'Internet: ${modeLabel(internet)}',
            child: const Icon(
              Icons.public_off,
              size: 15,
              color: Color(0xffb83b40),
            ),
          ),
        if (locked)
          const Tooltip(
            message: 'Màn hình đang khóa',
            child: Icon(Icons.lock, size: 15, color: Color(0xffdb8a22)),
          ),
        if (clipboardBlocked)
          const Tooltip(
            message: 'Đã chặn clipboard',
            child: Icon(
              Icons.content_paste_off,
              size: 15,
              color: Color(0xffdb8a22),
            ),
          ),
        const SizedBox(width: 4),
        dot,
      ],
    );
  }

  Widget _statusDot(Map<String, dynamic> r) {
    final Color color;
    if (r['online'] != true) {
      color = agent.seenOnlineIds.contains(r['id'])
          ? const Color(0xffd94848)
          : const Color(0xff525a63);
    } else if (mediaNeedsAttention(r)) {
      color = const Color(0xffdb8a22);
    } else if (r['monitoring'] != 'paused' &&
        r['monitoring'] != 'observing' &&
        (r['monitoring'] == 'unavailable' ||
            (r['thumbnail_age_ms'] as num? ?? 0) > 10000)) {
      color = const Color(0xffdb8a22);
    } else {
      color = const Color(0xff2b9c55);
    }
    return Tooltip(
      message: status(r),
      child: Icon(Icons.circle, size: 9, color: color),
    );
  }

  void toggle(String id) => setState(() {
    if (!selected.add(id)) {
      selected.remove(id);
    }
  });

  bool canDropTo(String id) =>
      !preparingFile &&
      agent.connected &&
      agent.students.any((row) => row['id'] == id && row['online'] == true) &&
      ((agent.state['tools'] as Map? ?? const {})['ready'] as List? ?? const [])
          .contains(id);

  Widget fileDrop(Map<String, dynamic> row, Widget child) {
    final id = row['id'] as String;
    return StudentFileDrop(
      key: ValueKey('file-drop-$id'),
      name: '${row['name']}',
      ip: '${row['ip']}',
      canSend: () => canDropTo(id),
      onSend: (items) async {
        setState(() => preparingFile = true);
        try {
          final limit =
              (((agent.state['settings'] as Map? ?? {})['tools'] as Map? ??
                              {})['max_file_mb']
                          as num? ??
                      300)
                  .toInt();
          await sendDroppedFiles(
            items,
            outbox: '${agent.endpoint.parent.path}/outbox',
            limit: limit * 1024 * 1024,
            active: () =>
                mounted &&
                agent.connected &&
                agent.students.any((r) => r['id'] == id && r['online'] == true),
            submit: (staged) => agent.commandChecked('tool_file', [id], staged),
          );
        } finally {
          if (mounted) setState(() => preparingFile = false);
        }
      },
      child: child,
    );
  }

  /// Offline computers get a clearly darker surface than the console
  /// background (0xfff3f5f9) and online cards (white).
  static const offlineSurface = Color(0xffd3dae1);

  Widget compact(Map<String, dynamic> r) {
    final id = r['id'] as String;
    return Material(
      color: selected.contains(id)
          ? const Color(0xffe1f0f2)
          : r['online'] == true
          ? Colors.white
          : offlineSurface,
      borderRadius: BorderRadius.circular(6),
      child: InkWell(
        onTap: () => selectionMode ? toggle(id) : inspect(r),
        onSecondaryTapDown: (d) => collectMenu(r, d.globalPosition),
        borderRadius: BorderRadius.circular(6),
        child: SizedBox(
          height: 48,
          child: Row(
            children: [
              if (selectionMode)
                Checkbox(
                  value: selected.contains(id),
                  onChanged: (_) => toggle(id),
                ),
              if (!selectionMode) const SizedBox(width: 14),
              connectionDot(r),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  '${r['name']}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
              ),
              if (mediaHealthLabel(r).isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(right: 12),
                  child: Text(
                    mediaHealthLabel(r),
                    style: const TextStyle(fontSize: 11),
                  ),
                ),
              SizedBox(
                width: 145,
                child: Text(
                  '${r['ip']}',
                  style: const TextStyle(color: Colors.blueGrey),
                ),
              ),
              moreButton(r),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> inspect(Map<String, dynamic> row) => agent.openObserve(row);

  Widget image(
    Map<String, dynamic> r, {
    bool flushTop = false,
    bool flat = false,
    bool crop = false,
  }) {
    final bytes = agent.images[r['id']];
    return ClipRRect(
      borderRadius: flat
          ? BorderRadius.zero
          : flushTop
          ? const BorderRadius.vertical(top: Radius.circular(13))
          : BorderRadius.circular(9),
      child: AspectRatio(
        aspectRatio: 16 / 9,
        child: ColoredBox(
          color: r['online'] == true
              ? const Color(0xffe8edf1)
              : const Color(0xffc6cfd8),
          child: bytes == null
              ? const Center(
                  child: Icon(
                    Icons.desktop_windows_outlined,
                    size: 40,
                    color: Colors.blueGrey,
                  ),
                )
              : Image.memory(
                  bytes,
                  fit: crop ? BoxFit.cover : BoxFit.contain,
                  filterQuality: FilterQuality.high,
                  gaplessPlayback: true,
                ),
        ),
      ),
    );
  }

  /// One discoverable entry point for the per-Student actions that are also
  /// available from the right-click menu.
  Widget moreButton(Map<String, dynamic> r) => Builder(
    builder: (context) => IconButton(
      tooltip: 'Thao tác với máy này (hoặc bấm chuột phải)',
      visualDensity: VisualDensity.compact,
      constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
      padding: const EdgeInsets.all(4),
      icon: const Icon(Icons.more_vert, size: 18),
      onPressed: r['online'] == true
          ? () {
              final box = context.findRenderObject() as RenderBox;
              collectMenu(
                r,
                box.localToGlobal(box.size.bottomLeft(Offset.zero)),
              );
            }
          : null,
    ),
  );

  Widget card(Map<String, dynamic> r, {bool list = false}) {
    final id = r['id'] as String;
    final chosen = selectionMode && selected.contains(id);
    final title = Row(
      children: [
        if (selectionMode)
          Checkbox(
            visualDensity: VisualDensity.compact,
            value: chosen,
            onChanged: (v) => setState(() {
              if (v == true) {
                selected.add(id);
              } else {
                selected.remove(id);
              }
            }),
          ),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '${r['name']}',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontWeight: FontWeight.w600,
                  fontSize: list ? 14 : 12,
                  height: 1.15,
                ),
              ),
              Text(
                '${r['ip']}',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: list ? 11 : 10,
                  height: 1.15,
                  color: Colors.blueGrey,
                ),
              ),
              if (list && mediaHealthLabel(r).isNotEmpty)
                Text(
                  mediaHealthLabel(r),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 11,
                    color: mediaNeedsAttention(r)
                        ? Colors.orange.shade800
                        : Colors.teal.shade700,
                  ),
                ),
            ],
          ),
        ),
        moreButton(r),
        connectionDot(r),
      ],
    );
    return Card(
      color: r['online'] == true ? null : offlineSurface,
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(14),
        side: BorderSide(
          color: chosen ? const Color(0xff156b79) : const Color(0xffdfe5eb),
          width: chosen ? 2 : 1,
        ),
      ),
      child: InkWell(
        borderRadius: BorderRadius.circular(14),
        onTap: () => selectionMode ? toggle(id) : inspect(r),
        onSecondaryTapDown: (d) => collectMenu(r, d.globalPosition),
        child: list
            ? Row(
                children: [
                  // Thumbnail bleeds to the card's top/left/bottom edges; the
                  // card's own clip rounds the corners.
                  SizedBox(width: 176, child: image(r, flat: true)),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.only(right: 12),
                      child: title,
                    ),
                  ),
                ],
              )
            : Stack(
                fit: StackFit.expand,
                children: [
                  image(r, flushTop: true, crop: true),
                  Positioned(
                    left: 0,
                    right: 0,
                    bottom: 0,
                    child: ColoredBox(
                      color: r['online'] == true
                          ? Colors.white.withValues(alpha: 0.84)
                          : offlineSurface.withValues(alpha: 0.92),
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(6, 3, 6, 3),
                        child: SizedBox(height: 28, child: title),
                      ),
                    ),
                  ),
                ],
              ),
      ),
    );
  }
}
