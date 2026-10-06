import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'agent.dart';
import 'observe.dart';
import 'observe_startup.dart';
import 'classroom_tools_ui.dart';
import 'settings.dart';
import 'file_transfer.dart';
import 'student_file_drop.dart';
import 'file_collection.dart';
import 'launch_apps.dart';
import 'power_controls.dart';
import 'internet_access.dart';
import 'activity_window.dart';
import 'files_window.dart';
import 'vscode_access.dart';

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
  const lifecycle = MethodChannel('classroom/lifecycle');
  lifecycle.setMethodCallHandler((call) async {
    if (call.method == 'reopenConsole' &&
        (observeId != null ||
            filesId != null ||
            activityId != null ||
            internetIds != null ||
            vscodeIds != null)) {
      await agent.command('show_console');
    }
  });
  Rect? observeBounds;
  if (observeId != null) {
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
  final theme = ThemeData(
    useMaterial3: true,
    fontFamily: 'ClassroomSystem',
    colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff156b79)),
    visualDensity: VisualDensity.compact,
  );
  final Widget home;
  final Size windowSize, minimumSize;
  final String title;
  if (filesId != null) {
    final name = agent.option('--files-name') ?? 'Student';
    home = FilesWindow(agent: agent, studentId: filesId, name: name);
    windowSize = const Size(1180, 720);
    minimumSize = const Size(820, 520);
    title = 'Files · $name';
  } else if (activityId != null) {
    final name = agent.option('--activity-name') ?? 'Student';
    home = ActivityWindow(
      agent: agent,
      studentId: activityId,
      studentName: name,
    );
    windowSize = const Size(1080, 760);
    minimumSize = const Size(760, 560);
    title = 'Activity · $name';
  } else if (internetIds != null) {
    final audience = agent.option('--internet-audience') ?? 'the class';
    home = InternetWindow(
      agent: agent,
      studentIds: internetIds.split(',').where((s) => s.isNotEmpty).toList(),
      audience: audience,
    );
    windowSize = const Size(980, 660);
    minimumSize = const Size(860, 560);
    title = 'Internet access · $audience';
  } else if (vscodeIds != null) {
    final audience = agent.option('--vscode-audience') ?? 'the class';
    home = VsCodeWindow(
      agent: agent,
      studentIds: vscodeIds.split(',').where((s) => s.isNotEmpty).toList(),
      audience: audience,
    );
    windowSize = const Size(1020, 720);
    minimumSize = const Size(860, 560);
    title = 'VS Code extensions · $audience';
  } else if (observeId != null) {
    final name = agent.option('--observe-name') ?? 'Student';
    home = ObserveWindow(agent: agent, studentId: observeId, name: name);
    windowSize = observeBounds?.size ?? const Size(1280, 800);
    minimumSize = Size(
      (observeBounds?.width ?? 800).clamp(0, 800),
      (observeBounds?.height ?? 600).clamp(0, 600),
    );
    title = 'Observe · $name';
  } else {
    home = ClassroomApp(agent: agent);
    windowSize = const Size(1280, 820);
    minimumSize = const Size(740, 520);
    title = 'Classroom · Teacher console';
  }
  final dependent =
      filesId != null ||
      activityId != null ||
      internetIds != null ||
      vscodeIds != null ||
      observeId != null;
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

class ClassroomApp extends StatelessWidget {
  const ClassroomApp({super.key, required this.agent});
  final Agent agent;
  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    title: 'Classroom',
    theme: ThemeData(
      useMaterial3: true,
      fontFamily: 'ClassroomSystem',
      colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff156b79)),
      scaffoldBackgroundColor: const Color(0xfff3f5f9),
      visualDensity: VisualDensity.compact,
      cardTheme: const CardThemeData(elevation: 0, margin: EdgeInsets.zero),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(10),
          ),
        ),
      ),
    ),
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

  Future<void> internetAccess() {
    final ids = toolRecipients;
    if (ids.isEmpty) return Future.value();
    final audience = selectionMode && selected.isNotEmpty
        ? '${ids.length} selected computers'
        : 'the class (${ids.length} online)';
    return agent.request(
      'internet_window',
      selected: ids,
      input: {'audience': audience},
    );
  }

  Future<void> vscodeAccess() {
    final ids = toolRecipients;
    if (ids.isEmpty) return Future.value();
    final audience = selectionMode && selected.isNotEmpty
        ? '${ids.length} selected computers'
        : 'the class (${ids.length} online)';
    return agent.request(
      'vscode_window',
      selected: ids,
      input: {'audience': audience},
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
          ? '${ids.length} selected computers'
          : 'the class (${ids.length} online)',
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
          : 'the class (${ids.length} online)',
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
          : 'the class (${ids.length} online)',
    );
  }

  Future<void> collectMenu(Map<String, dynamic> row, Offset at) async {
    if (row['online'] != true) return;
    final choice = await showMenu<String>(
      context: context,
      position: RelativeRect.fromLTRB(at.dx, at.dy, at.dx, at.dy),
      items: const [
        PopupMenuItem(value: 'files', child: Text('Collect files…')),
        PopupMenuItem(value: 'observe', child: Text('Observe screen')),
      ],
    );
    if (choice == 'files') {
      await agent.openFiles(row);
    } else if (choice == 'observe') {
      await inspect(row);
    }
  }

  Future<void> sendClassMessage() async {
    final controller = TextEditingController();
    final text = await showDialog<String>(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: Text(
            toolRecipients.isEmpty
                ? 'Message the class'
                : 'Message ${toolRecipients.length} selected students',
          ),
          content: SizedBox(
            width: 420,
            child: TextField(
              controller: controller,
              autofocus: true,
              maxLines: 5,
              maxLength: 2000,
              decoration: const InputDecoration(
                hintText: 'Write a message for the student screen',
                border: OutlineInputBorder(),
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, controller.text),
              child: const Text('Send'),
            ),
          ],
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
                  ? e.message ?? 'Could not stage the file'
                  : 'Could not prepare the file: $e',
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
    windowManager.addListener(this);
    windowManager.setPreventClose(true);
    if (Platform.isMacOS) {
      permissions
          .invokeMethod<bool>('screenRecordingGranted')
          .then((granted) {
            if (mounted) {
              setState(() => screenPermissionNeeded = granted == false);
            }
          })
          .catchError((Object _) {
            /* Agent capture errors remain authoritative. */
          });
    }
  }

  bool loadedView = false;
  bool wasSharing = false;
  bool havePresentationState = false;
  bool screenPermissionNeeded = false;
  static const permissions = MethodChannel('classroom/permissions');
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
    if (havePresentationState && sharing && !wasSharing && n <= request) {
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
    final showPermission =
        Platform.isMacOS && (screenPermissionNeeded || capturePermissionError);
    final phase = agent.state['phase'] ?? 'connecting';
    final idle = phase == 'idle' && agent.connected && !agent.commandBusy;
    final onlineSelected = agent.students
        .where((r) => r['online'] == true && selected.contains(r['id']))
        .map((r) => r['id'] as String)
        .toSet();
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
                        'Teacher',
                        style: TextStyle(
                          color: Color(0xffaabdc9),
                          fontSize: 11,
                        ),
                      ),
                    const Spacer(),
                    Text(
                      MediaQuery.sizeOf(context).width < 900
                          ? '$online / ${agent.students.length} online'
                          : '$online online / ${agent.students.length} computers',
                      style: const TextStyle(
                        color: Color(0xffc2d0d9),
                        fontSize: 12,
                      ),
                    ),
                    if (selected.isNotEmpty &&
                        MediaQuery.sizeOf(context).width >= 900)
                      Padding(
                        padding: const EdgeInsets.only(left: 12),
                        child: Text(
                          '${selected.length} selected',
                          style: const TextStyle(
                            color: Colors.white,
                            fontSize: 12,
                          ),
                        ),
                      ),
                    if (!agent.connected ||
                        phase != 'idle' ||
                        agent.commandBusy)
                      Padding(
                        padding: const EdgeInsets.only(left: 14),
                        child: Text(
                          !agent.connected
                              ? 'Connecting…'
                              : agent.commandBusy
                              ? 'Working…'
                              : phase == 'sharing'
                              ? 'Sharing screen'
                              : phase == 'starting'
                              ? 'Starting…'
                              : 'Stopping…',
                          style: const TextStyle(
                            color: Color(0xff87d4dc),
                            fontSize: 12,
                          ),
                        ),
                      ),
                    const SizedBox(width: 10),
                    SizedBox(
                      width: 28,
                      height: 28,
                      child: IconButton(
                        padding: EdgeInsets.zero,
                        tooltip: 'Classroom settings',
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
                        tooltip: 'Exit Classroom',
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
                    final actions = Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (agent.connected && phase == 'idle') ...[
                          FilledButton.icon(
                            style: toolbarButton(),
                            onPressed: idle && online > 0
                                ? () => agent.command('share_class')
                                : null,
                            icon: const Icon(Icons.cast, size: 16),
                            label: const Text('Share class'),
                          ),
                          if (onlineSelected.isNotEmpty) ...[
                            const SizedBox(width: 8),
                            OutlinedButton.icon(
                              style: toolbarButton(),
                              onPressed: idle
                                  ? () => agent.command(
                                      'share_selected',
                                      onlineSelected,
                                    )
                                  : null,
                              icon: const Icon(
                                Icons.screen_share_outlined,
                                size: 16,
                              ),
                              label: Text(
                                'Share selected (${onlineSelected.length})',
                              ),
                            ),
                          ],
                        ],
                        if (agent.connected &&
                            (phase == 'sharing' || phase == 'starting'))
                          FilledButton.icon(
                            style: toolbarButton().copyWith(
                              backgroundColor: const WidgetStatePropertyAll(
                                Color(0xffb83b40),
                              ),
                            ),
                            onPressed: agent.commandBusy
                                ? null
                                : () => agent.command('stop'),
                            icon: const Icon(Icons.stop, size: 17),
                            label: const Text('Stop sharing'),
                          ),
                        if (agent.connected && phase == 'stopping')
                          const Text(
                            'Finishing presentation…',
                            style: TextStyle(
                              fontSize: 12,
                              color: Colors.blueGrey,
                            ),
                          ),
                      ],
                    );
                    final tools = Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (agent.connected) ...[
                          IconButton(
                            tooltip: selectionMode && onlineSelected.isNotEmpty
                                ? 'Message selected students'
                                : 'Message the class',
                            visualDensity: VisualDensity.compact,
                            onPressed: !agent.commandBusy && canUseTools
                                ? sendClassMessage
                                : null,
                            icon: const Icon(Icons.chat_outlined, size: 18),
                          ),
                          IconButton(
                            tooltip: selectionMode && onlineSelected.isNotEmpty
                                ? 'Send file to selected students'
                                : 'Send file to the class',
                            visualDensity: VisualDensity.compact,
                            onPressed: !agent.commandBusy && canUseTools
                                ? sendClassFile
                                : null,
                            icon: const Icon(Icons.attach_file, size: 18),
                          ),
                          IconButton(
                            tooltip: selectionMode && onlineSelected.isNotEmpty
                                ? 'Launch an app on selected students'
                                : 'Launch an app on the class',
                            visualDensity: VisualDensity.compact,
                            onPressed: !agent.commandBusy && canUseTools
                                ? launchApps
                                : null,
                            icon: const Icon(
                              Icons.rocket_launch_outlined,
                              size: 18,
                            ),
                          ),
                          AnimatedBuilder(
                            animation: agent,
                            builder: (context, _) => IconButton(
                              tooltip: activityMonitoringOn
                                  ? 'Activity monitoring is on — Students collect open windows, browser tabs and downloads in the background. Tap to turn off.'
                                  : 'Activity monitoring is off — Students are not collecting this. Tap to turn on for the whole class.',
                              visualDensity: VisualDensity.compact,
                              isSelected: activityMonitoringOn,
                              onPressed: !agent.commandBusy
                                  ? toggleActivityMonitoring
                                  : null,
                              icon: const Icon(
                                Icons.visibility_outlined,
                                size: 18,
                              ),
                              selectedIcon: const Icon(
                                Icons.visibility,
                                size: 18,
                                color: Color(0xff156b79),
                              ),
                            ),
                          ),
                          AnimatedBuilder(
                            animation: agent,
                            builder: (context, _) => IconButton(
                              tooltip: 'Internet access',
                              visualDensity: VisualDensity.compact,
                              onPressed: !agent.commandBusy && canUseTools
                                  ? internetAccess
                                  : null,
                              icon: Badge(
                                isLabelVisible: restrictedStudents(
                                  agent,
                                ).isNotEmpty,
                                label: Text(
                                  '${restrictedStudents(agent).length}',
                                ),
                                child: const Icon(
                                  Icons.public_off_outlined,
                                  size: 18,
                                ),
                              ),
                            ),
                          ),
                          AnimatedBuilder(
                            animation: agent,
                            builder: (context, _) => IconButton(
                              tooltip: 'VS Code extensions',
                              visualDensity: VisualDensity.compact,
                              onPressed: !agent.commandBusy && canUseTools
                                  ? vscodeAccess
                                  : null,
                              icon: Badge(
                                isLabelVisible: vscodeBlockedStudents(
                                  agent,
                                ).isNotEmpty,
                                label: Text(
                                  '${vscodeBlockedStudents(agent).length}',
                                ),
                                child: const Icon(
                                  Icons.extension_outlined,
                                  size: 18,
                                ),
                              ),
                            ),
                          ),
                          AnimatedBuilder(
                            animation: agent,
                            builder: (context, _) => PopupMenuButton<String>(
                              tooltip: 'Lock, log out, restart or shut down',
                              enabled: !agent.commandBusy && canUseTools,
                              padding: EdgeInsets.zero,
                              icon: Badge(
                                isLabelVisible:
                                    lockedIds.isNotEmpty ||
                                    clipboardBlockedIds.isNotEmpty,
                                label: Text(
                                  '${lockedIds.length + clipboardBlockedIds.length}',
                                ),
                                child: const Icon(
                                  Icons.power_settings_new,
                                  size: 18,
                                ),
                              ),
                              onSelected: powerAction,
                              itemBuilder: (_) => powerMenuItems(
                                anyLocked: lockedIds.isNotEmpty,
                                anyClipboardBlocked:
                                    clipboardBlockedIds.isNotEmpty,
                              ),
                            ),
                          ),
                          AnimatedBuilder(
                            animation: agent,
                            builder: (context, _) => PopupMenuButton<String>(
                              tooltip:
                                  selectionMode && onlineSelected.isNotEmpty
                                  ? 'Collect files from selected students'
                                  : 'Collect files from the class',
                              enabled: !agent.commandBusy && canUseTools,
                              padding: EdgeInsets.zero,
                              icon: Badge(
                                isLabelVisible: agent.fileJobs.any(
                                  (j) =>
                                      j['kind'] == 'collect' &&
                                      (j['targets'] as List).any(
                                        (t) => jobTargetActive(
                                          Map<String, dynamic>.from(t as Map),
                                        ),
                                      ),
                                ),
                                smallSize: 8,
                                child: const Icon(
                                  Icons.download_for_offline_outlined,
                                  size: 18,
                                ),
                              ),
                              onSelected: (action) {
                                if (action == 'new') {
                                  collectFiles();
                                } else {
                                  showCollectProgress(context, agent);
                                }
                              },
                              itemBuilder: (_) => [
                                const PopupMenuItem(
                                  value: 'new',
                                  child: Text('Collect files…'),
                                ),
                                PopupMenuItem(
                                  value: 'progress',
                                  enabled: agent.fileJobs.any(
                                    (j) => j['kind'] == 'collect',
                                  ),
                                  child: const Text('Collection progress'),
                                ),
                              ],
                            ),
                          ),
                          AnimatedBuilder(
                            animation: toolActivity,
                            builder: (context, _) => IconButton(
                              visualDensity: VisualDensity.compact,
                              tooltip: toolActivity.unread == 0
                                  ? 'Student replies'
                                  : '${toolActivity.unread} new replies',
                              onPressed: () =>
                                  showRepliesDialog(context, toolActivity),
                              icon: Badge(
                                isLabelVisible: toolActivity.unread > 0,
                                label: Text('${toolActivity.unread}'),
                                child: const Icon(
                                  Icons.forum_outlined,
                                  size: 18,
                                ),
                              ),
                            ),
                          ),
                          AnimatedBuilder(
                            animation: toolActivity,
                            builder: (context, _) => IconButton(
                              visualDensity: VisualDensity.compact,
                              tooltip: toolActivity.disconnects.isEmpty
                                  ? 'Disconnect alerts'
                                  : '${toolActivity.disconnects.length} disconnect alert${toolActivity.disconnects.length == 1 ? '' : 's'}',
                              onPressed: () =>
                                  showDisconnectsDialog(context, toolActivity),
                              icon: Badge(
                                isLabelVisible:
                                    toolActivity.unreadDisconnects > 0,
                                backgroundColor: const Color(0xffb83b40),
                                label: Text(
                                  '${toolActivity.unreadDisconnects}',
                                ),
                                child: const Icon(Icons.link_off, size: 18),
                              ),
                            ),
                          ),
                        ],
                        SizedBox(
                          width: constraints.maxWidth < 900 ? 170 : 220,
                          height: 32,
                          child: TextField(
                            onChanged: (v) => setState(() => query = v),
                            style: const TextStyle(fontSize: 12),
                            decoration: InputDecoration(
                              hintText: 'Find name or IP',
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
                        const SizedBox(width: 8),
                        SizedBox(
                          width: 30,
                          height: 32,
                          child: IconButton(
                            padding: EdgeInsets.zero,
                            tooltip: 'Rescan connections',
                            onPressed: agent.connected && !agent.commandBusy
                                ? agent.rescan
                                : null,
                            icon: const Icon(Icons.refresh, size: 18),
                          ),
                        ),
                        SizedBox(
                          width: 32,
                          height: 32,
                          child: IconButton(
                            padding: EdgeInsets.zero,
                            tooltip: selectionMode
                                ? 'Finish selecting'
                                : 'Select computers for presentation',
                            onPressed: () => setState(() {
                              selectionMode = !selectionMode;
                              if (!selectionMode) selected.clear();
                            }),
                            icon: Icon(
                              Icons.checklist,
                              size: 18,
                              color: selectionMode
                                  ? const Color(0xff156b79)
                                  : Colors.blueGrey,
                            ),
                          ),
                        ),
                        if (selectionMode)
                          PopupMenuButton<String>(
                            tooltip: 'Selection',
                            padding: EdgeInsets.zero,
                            icon: const Icon(Icons.more_horiz, size: 18),
                            constraints: const BoxConstraints(minWidth: 180),
                            onSelected: (action) => setState(() {
                              if (action == 'all') {
                                selected.addAll(
                                  rows
                                      .where((r) => r['online'] == true)
                                      .map((r) => r['id'] as String),
                                );
                              } else {
                                selected.clear();
                              }
                            }),
                            itemBuilder: (_) => [
                              PopupMenuItem(
                                value: 'all',
                                enabled: rows.any((r) => r['online'] == true),
                                child: const Text('Select visible online'),
                              ),
                              PopupMenuItem(
                                value: 'clear',
                                enabled: selected.isNotEmpty,
                                child: const Text('Clear selection'),
                              ),
                            ],
                          ),
                        const SizedBox(width: 6),
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
                              message: 'Grid view',
                              child: Icon(Icons.grid_view, size: 16),
                            ),
                            Tooltip(
                              message: 'List view',
                              child: Icon(Icons.view_list_outlined, size: 17),
                            ),
                            Tooltip(
                              message: 'Compact list',
                              child: Icon(Icons.format_list_bulleted, size: 17),
                            ),
                          ],
                        ),
                      ],
                    );
                    if (constraints.maxWidth < 940 &&
                        onlineSelected.isNotEmpty &&
                        phase == 'idle') {
                      return Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          actions,
                          const SizedBox(height: 6),
                          Align(alignment: Alignment.centerRight, child: tools),
                        ],
                      );
                    }
                    return Row(children: [actions, const Spacer(), tools]);
                  },
                ),
              ),
              if (agent.error != null || showPermission)
                MaterialBanner(
                  content: Text(
                    showPermission
                        ? 'Allow Classroom Console to record the screen, then restart the app.'
                        : agent.error!,
                  ),
                  actions: [
                    if (showPermission)
                      FilledButton.icon(
                        onPressed: () async {
                          try {
                            await permissions.invokeMethod<bool>(
                              'openScreenRecordingSettings',
                            );
                          } catch (_) {
                            if (context.mounted) {
                              ScaffoldMessenger.of(context).showSnackBar(
                                const SnackBar(
                                  content: Text(
                                    'Open System Settings → Privacy & Security → Screen Recording.',
                                  ),
                                ),
                              );
                            }
                          }
                        },
                        icon: const Icon(Icons.settings_outlined, size: 16),
                        label: const Text('Open Settings'),
                      ),
                    TextButton(
                      onPressed: () {
                        setState(() => screenPermissionNeeded = false);
                        if (agent.connected) {
                          agent.command('clear_error');
                        } else {
                          agent.start();
                        }
                      },
                      child: Text(agent.connected ? 'Dismiss' : 'Reconnect'),
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
                                  ? (query.isEmpty
                                        ? 'Waiting for students'
                                        : 'No matching computers')
                                  : 'Connecting to classroom…',
                              style: const TextStyle(
                                fontSize: 21,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(height: 8),
                            const Text(
                              'Connected computers will appear here automatically.',
                            ),
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
                  vertical: 10,
                ),
                child: Row(
                  children: [
                    Icon(
                      agent.connected ? Icons.link : Icons.link_off,
                      size: 16,
                      color: Colors.blueGrey,
                    ),
                    const SizedBox(width: 8),
                    Text(
                      agent.connected
                          ? 'Teacher ${agent.state['teacher_ip']}'
                          : 'Agent disconnected',
                    ),
                    const Spacer(),
                    if (view == 'grid') ...[
                      const Icon(
                        Icons.photo_size_select_large,
                        size: 15,
                        color: Colors.blueGrey,
                      ),
                      SizedBox(
                        width: MediaQuery.sizeOf(context).width < 900
                            ? 120
                            : 160,
                        child: Slider(
                          value: gridSize,
                          min: 180,
                          max: 480,
                          divisions: 15,
                          label: '${gridSize.round()} px',
                          semanticFormatterCallback: (v) =>
                              'Grid item size ${v.round()} pixels',
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
                          'Auto',
                          style: TextStyle(fontSize: 11),
                        ),
                        selected: gridAuto,
                        visualDensity: VisualDensity.compact,
                        onSelected: (value) {
                          setState(() => gridAuto = value);
                          agent.savePreferences({
                            'grid_auto': gridAuto,
                            'grid_size': gridSize,
                          });
                        },
                      ),
                    ] else
                      Flexible(
                        child: Text(
                          agent.notice ?? 'Classroom · Local network',
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            color: Colors.blueGrey,
                            fontSize: 11,
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
          Positioned(
            right: 20,
            bottom: 58,
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
      mainAxisExtent: itemWidth * 9 / 16 + 64,
    );
  }

  int gridColumns(double width, int count, double height) {
    if (gridAuto) {
      var columns = ((width + 16) / 316).round().clamp(1, count.clamp(1, 12));
      // Avoid an oversized single card hiding its title/actions below the viewport.
      final fittingWidth = (height - 64).clamp(100, double.infinity) * 16 / 9;
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
          ? 'Connection lost'
          : 'Not connected in this session';
    }
    if (r['monitoring'] == 'observing') {
      return 'Live Observe active';
    }
    if (r['monitoring'] == 'paused') {
      return 'Viewing presentation';
    }
    if (r['monitoring'] == 'unavailable') {
      return 'Screen snapshot unavailable';
    }
    if ((r['thumbnail_age_ms'] as num? ?? 0) > 10000) {
      return 'Screen snapshot outdated';
    }
    return 'Online';
  }

  Widget connectionDot(Map<String, dynamic> r) {
    final dot = _statusDot(r);
    final internet = restrictedStudents(agent)['${r['id']}'];
    final locked = lockedIds.contains(r['id']);
    final clipboardBlocked = clipboardBlockedIds.contains(r['id']);
    if (!locked && !clipboardBlocked && internet == null) return dot;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
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
            message: 'Screen locked',
            child: Icon(Icons.lock, size: 15, color: Color(0xffdb8a22)),
          ),
        if (clipboardBlocked)
          const Tooltip(
            message: 'Clipboard blocked',
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

  Widget compact(Map<String, dynamic> r) {
    final id = r['id'] as String;
    return Material(
      color: selected.contains(id) ? const Color(0xffe1f0f2) : Colors.white,
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
              SizedBox(
                width: 145,
                child: Text(
                  '${r['ip']}',
                  style: const TextStyle(color: Colors.blueGrey),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> inspect(Map<String, dynamic> row) => agent.openObserve(row);

  Widget image(Map<String, dynamic> r, {bool flushTop = false}) {
    final bytes = agent.images[r['id']];
    return ClipRRect(
      borderRadius: flushTop
          ? const BorderRadius.vertical(top: Radius.circular(13))
          : BorderRadius.circular(9),
      child: AspectRatio(
        aspectRatio: 16 / 9,
        child: ColoredBox(
          color: const Color(0xffe8edf1),
          child: bytes == null
              ? const Center(
                  child: Icon(
                    Icons.desktop_windows_outlined,
                    size: 40,
                    color: Colors.blueGrey,
                  ),
                )
              : Image.memory(bytes, fit: BoxFit.cover, gaplessPlayback: true),
        ),
      ),
    );
  }

  Widget card(Map<String, dynamic> r, {bool list = false}) {
    final id = r['id'] as String;
    final chosen = selectionMode && selected.contains(id);
    final title = Row(
      children: [
        if (selectionMode)
          Checkbox(
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
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
              Text(
                '${r['ip']}',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontSize: 11, color: Colors.blueGrey),
              ),
            ],
          ),
        ),
        IconButton(
          tooltip: 'Browse and collect files',
          icon: const Icon(Icons.folder_copy_outlined, size: 17),
          onPressed: r['online'] == true ? () => agent.openFiles(r) : null,
        ),
        IconButton(
          tooltip: 'Open windows, browser tabs and downloads',
          icon: const Icon(Icons.visibility_outlined, size: 17),
          onPressed: r['online'] == true ? () => agent.openActivity(r) : null,
        ),
        IconButton(
          tooltip: 'Inspect screen',
          icon: const Icon(Icons.open_in_full, size: 17),
          onPressed: () => inspect(r),
        ),
        connectionDot(r),
      ],
    );
    return Card(
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
            ? Padding(
                padding: const EdgeInsets.all(12),
                child: Row(
                  children: [
                    SizedBox(width: 160, child: image(r)),
                    const SizedBox(width: 20),
                    Expanded(child: title),
                  ],
                ),
              )
            : Column(
                children: [
                  Expanded(child: image(r, flushTop: true)),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(10, 6, 10, 4),
                    child: SizedBox(height: 44, child: title),
                  ),
                ],
              ),
      ),
    );
  }
}
