import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'agent.dart';

class TeacherSettings {
  TeacherSettings(this.ip, this.port, this.overlay);
  final String ip;
  final int port;
  final Map<String, dynamic> overlay;
}

class TeacherSettingsDialog extends StatefulWidget {
  const TeacherSettingsDialog({
    super.key,
    required this.agent,
    this.selected = const [],
  });
  final Agent agent;
  final List<String> selected;
  @override
  State<TeacherSettingsDialog> createState() => _TeacherSettingsDialogState();
}

class _TeacherSettingsDialogState extends State<TeacherSettingsDialog> {
  static const sections = [
    'Connection',
    'Broadcast',
    'Observe',
    'Diagnostics',
    'Student updates',
  ];
  int page = 0;
  String? error;
  bool preparingUpdate = false;
  late final TextEditingController ip, port, adapter;
  int resolution = 1920, fps = 30, observeFps = 60, interval = 2000;
  double bitrate = 6;
  bool adaptive = true, enhanced = true;
  bool autoCollect = false;
  String codec = 'auto', level = 'info';
  Map get effective => widget.agent.state['settings'] as Map? ?? const {};
  Map section(String key) => effective[key] as Map? ?? const {};

  @override
  void initState() {
    super.initState();
    final network = section('network');
    final address =
        widget.agent.option('--bind-ip') ??
        widget.agent.preferences['bind_ip'] ??
        network['bind_ip'] ??
        'auto';
    ip = TextEditingController(text: address == 'auto' ? '' : '$address');
    port = TextEditingController(
      text:
          '${widget.agent.state['control_port'] ?? widget.agent.preferences['port'] ?? 47800}',
    );
    final interface = network['interface'] ?? 'auto_default_route_ethernet';
    adapter = TextEditingController(
      text: interface == 'auto_default_route_ethernet' ? '' : '$interface',
    );
    final video = section('video');
    resolution = [1920, 1600, 1280].contains(video['width'])
        ? video['width'] as int
        : 1920;
    fps = [15, 20, 24, 30].contains(video['max_fps'])
        ? video['max_fps'] as int
        : 30;
    bitrate = ((video['target_bitrate_kbps'] as num? ?? 6000) / 1000).clamp(
      1.5,
      8,
    );
    adaptive = video['adaptive'] as bool? ?? true;
    final observe = section('observe');
    enhanced = observe['enhanced'] as bool? ?? true;
    codec = '${observe['codec'] ?? 'auto'}';
    observeFps = observe['max_fps'] == 30 ? 30 : 60;
    interval =
        ((section('monitoring')['interval_ms'] as num? ?? 2000) ~/ 1000).clamp(
          1,
          10,
        ) *
        1000;
    level = '${section('app')['log_level'] ?? 'info'}';
    autoCollect = section('diagnostics')['auto_collect'] as bool? ?? false;
    if (!['auto', 'h264', 'h265'].contains(codec)) codec = 'auto';
    if (!['info', 'debug', 'warn'].contains(level)) level = 'info';
  }

  @override
  void dispose() {
    ip.dispose();
    port.dispose();
    adapter.dispose();
    super.dispose();
  }

  Widget field(
    TextEditingController controller,
    String label, {
    String? hint,
    bool enabled = true,
  }) => Padding(
    padding: const EdgeInsets.only(bottom: 16),
    child: TextField(
      controller: controller,
      enabled: enabled,
      decoration: InputDecoration(
        labelText: label,
        hintText: hint,
        border: const OutlineInputBorder(),
      ),
    ),
  );

  Widget choice<T>(
    String label,
    T value,
    Map<T, String> options,
    void Function(T) change,
  ) => Padding(
    padding: const EdgeInsets.only(bottom: 16),
    child: DropdownButtonFormField<T>(
      initialValue: value,
      isExpanded: true,
      decoration: InputDecoration(
        labelText: label,
        border: const OutlineInputBorder(),
      ),
      items: [
        for (final entry in options.entries)
          DropdownMenuItem(value: entry.key, child: Text(entry.value)),
      ],
      onChanged: (value) {
        if (value != null) setState(() => change(value));
      },
    ),
  );

  Future<void> showLogs({bool diagnostics = false}) async {
    final dir = widget.agent.dataDirectory;
    if (dir == null) return;
    try {
      final folder = diagnostics
          ? Directory('${dir.path}${Platform.pathSeparator}diagnostics')
          : dir;
      await folder.create(recursive: true);
      await Process.run(Platform.isMacOS ? 'open' : 'explorer.exe', [
        folder.path,
      ]);
    } catch (e) {
      if (mounted) setState(() => error = 'Could not open logs: $e');
    }
  }

  Future<void> updateStudents() async {
    setState(() {
      preparingUpdate = true;
      error = null;
    });
    try {
      final picked = await const MethodChannel('classroom/files')
          .invokeMapMethod<String, dynamic>('chooseFile', {
            'directory': widget.agent.endpoint.parent.path,
          });
      if (picked == null) return;
      await widget.agent.command('tool_update', widget.selected, picked);
    } catch (e) {
      if (mounted) setState(() => error = 'Could not start dev update: $e');
    } finally {
      if (mounted) setState(() => preparingUpdate = false);
    }
  }

  void save() {
    final n = int.tryParse(port.text.trim());
    final address = ip.text.trim();
    if (n == null ||
        n < 1024 ||
        n > 65524 ||
        (address.isNotEmpty &&
            address != 'auto' &&
            InternetAddress.tryParse(address)?.type !=
                InternetAddressType.IPv4)) {
      setState(() {
        page = 0;
        error =
            'Enter an IPv4 address (or leave blank) and a port from 1024 to 65524.';
      });
      return;
    }
    Navigator.pop(
      context,
      TeacherSettings(address.isEmpty ? 'auto' : address, n, {
        'app': {...section('app'), 'log_level': level},
        'network': {
          ...section('network'),
          'interface': adapter.text.trim().isEmpty
              ? 'auto_default_route_ethernet'
              : adapter.text.trim(),
        },
        'video': {
          ...section('video'),
          'width': resolution,
          'height': resolution == 1920
              ? 1080
              : resolution == 1600
              ? 900
              : 720,
          'max_fps': fps,
          'adaptive_idle_fps': 5,
          'adaptive_typing_fps': fps < 24 ? fps : 24,
          'target_bitrate_kbps': (bitrate * 1000).round(),
          'min_bitrate_kbps': 1500,
          'max_bitrate_kbps': 8000,
          'adaptive': adaptive,
        },
        'monitoring': {...section('monitoring'), 'interval_ms': interval},
        'observe': {
          ...section('observe'),
          'enhanced': enhanced,
          'codec': codec,
          'max_fps': observeFps,
        },
        'diagnostics': {'auto_collect': autoCollect},
      }),
    );
  }

  @override
  Widget build(BuildContext context) {
    final panels = <List<Widget>>[
      [
        const Text(
          'Students connect to this computer. Use the same control port on every machine.',
        ),
        const SizedBox(height: 20),
        field(
          ip,
          'Teacher local IPv4',
          hint: 'Automatic',
          enabled: widget.agent.option('--bind-ip') == null,
        ),
        field(
          port,
          'Control port',
          enabled: widget.agent.option('--control-port') == null,
        ),
        field(
          adapter,
          'Network adapter name',
          hint: 'Automatic: active Ethernet / default route',
        ),
        Text(
          'Current address: ${widget.agent.state['teacher_ip'] ?? 'connecting'}',
        ),
        const SizedBox(height: 12),
        const Text(
          'Automatic selection avoids inactive virtual adapters. A manual IP must belong to this computer.',
        ),
        if (widget.agent.option('--bind-ip') != null ||
            widget.agent.option('--control-port') != null)
          const Text(
            'IP / port supplied at launch take precedence over saved settings.',
          ),
      ],
      [
        const Text(
          'Preserve readable text and fresh frames. Automatic quality adapts bitrate and frame rate under pressure.',
        ),
        const SizedBox(height: 20),
        choice('Maximum resolution', resolution, {
          1920: '1920 × 1080',
          1600: '1600 × 900',
          1280: '1280 × 720',
        }, (v) => resolution = v),
        choice('Maximum frame rate', fps, {
          15: '15 fps',
          20: '20 fps',
          24: '24 fps',
          30: '30 fps',
        }, (v) => fps = v),
        Text('Target bitrate: ${bitrate.toStringAsFixed(1)} Mbps'),
        Slider(
          value: bitrate,
          min: 1.5,
          max: 8,
          divisions: 13,
          label: '${bitrate.toStringAsFixed(1)} Mbps',
          onChanged: (v) => setState(() => bitrate = v),
        ),
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: const Text('Automatic quality'),
          value: adaptive,
          onChanged: (v) => setState(() => adaptive = v),
        ),
        const Text(
          'Source aspect ratio is preserved; smaller screens are never upscaled. The Teacher pointer remains visible to Students.',
        ),
      ],
      [
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: const Text('Enhanced one-to-one Observe'),
          subtitle: const Text('Use mutually supported hardware capabilities.'),
          value: enhanced,
          onChanged: (v) => setState(() => enhanced = v),
        ),
        choice('Preferred codec', codec, {
          'auto': 'Automatic',
          'h264': 'H.264',
          'h265': 'H.265 when supported',
        }, (v) => codec = v),
        choice('Observe frame-rate ceiling', observeFps, {
          30: '30 fps',
          60: '60 fps when supported',
        }, (v) => observeFps = v),
        choice('Roster thumbnail refresh', interval, {
          for (var i = 1; i <= 10; i++) i * 1000: '$i seconds',
        }, (v) => interval = v),
        const Text(
          'Unsupported profiles fall back to H.264. Each Observe window opens in view-only mode; Control is an explicit action.',
        ),
      ],
      [
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: const Text('Collect automatically after Stop'),
          value: autoCollect,
          onChanged: (v) => setState(() => autoCollect = v),
        ),
        FilledButton.icon(
          icon: const Icon(Icons.archive_outlined),
          label: Text(
            widget.selected.isEmpty
                ? 'Collect diagnostics · whole class'
                : 'Collect diagnostics · ${widget.selected.length} selected',
          ),
          onPressed: () async {
            try {
              await widget.agent.command(
                'collect_diagnostics',
                widget.selected,
              );
            } catch (e) {
              if (mounted) setState(() => error = '$e');
            }
          },
        ),
        AnimatedBuilder(
          animation: widget.agent,
          builder: (_, _) {
            final status =
                widget.agent.state['diagnostics'] as Map? ?? const {};
            return Padding(
              padding: const EdgeInsets.symmetric(vertical: 12),
              child: SelectableText(
                status['phase'] == 'complete'
                    ? 'Collected ${status['collected']}/${status['total']} Students; ${status['missing']} unavailable.\n${status['path']}\n${(status['students'] as List? ?? []).where((s) => s['status'] != 'collected').map((s) => "${s['name']} (${s['ip']}): ${s['reason']}").join('\n')}'
                    : status['phase'] == 'waiting_for_stop'
                    ? 'Queued: collection starts after Stop.'
                    : status['phase'] == 'error'
                    ? 'Collection failed: ${status['error']}'
                    : status['phase'] == 'collecting'
                    ? 'Collecting: ${status['completed'] ?? 0}/${status['total']} Students'
                    : 'Ready to collect Teacher and Student logs.',
              ),
            );
          },
        ),
        OutlinedButton.icon(
          onPressed: () => showLogs(diagnostics: true),
          icon: const Icon(Icons.folder_zip_outlined),
          label: const Text('Open diagnostics folder'),
        ),
        const Text(
          'Collection waits until Stop, uses a separate connection limited to 512 KiB/s, and pauses when a new presentation starts. Each ZIP includes build, configuration and machine identity. Screenshots, message/file contents and IPC tokens are excluded.',
        ),
        const SizedBox(height: 16),
        choice('Log detail', level, {
          'info': 'Normal',
          'debug': 'Detailed',
          'warn': 'Warnings and errors',
        }, (v) => level = v),
        OutlinedButton.icon(
          onPressed: showLogs,
          icon: const Icon(Icons.folder_open),
          label: const Text('Open logs folder'),
        ),
        if (Platform.isMacOS)
          OutlinedButton(
            onPressed: () async {
              await const MethodChannel(
                'classroom/permissions',
              ).invokeMethod<void>('openScreenRecordingSettings');
            },
            child: const Text('Screen Recording permissions'),
          ),
        const SizedBox(height: 16),
        Text('Teacher · ${Platform.operatingSystem}'),
        const Text(
          'Closing the console hides it in the tray. Exit Classroom closes all windows and media processes.',
        ),
      ],
      [
        const Text(
          'Student updates · Development',
          style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 12),
        const Text(
          'Choose a dev-update ZIP made with classroom-dev-update.py. '
          'The Student receives, verifies, restarts and reconnects automatically. '
          'Stop sharing and close Observe first. Configuration and logs are preserved.',
        ),
        const SizedBox(height: 16),
        FilledButton.icon(
          onPressed: preparingUpdate ? null : updateStudents,
          icon: const Icon(Icons.system_update_alt),
          label: Text(
            preparingUpdate
                ? 'Preparing package…'
                : widget.selected.isEmpty
                ? 'Update compatible Students…'
                : 'Update ${widget.selected.length} selected…',
          ),
        ),
        const SizedBox(height: 12),
        const Text(
          'An updater-enabled Student must be installed once manually. '
          'macOS may still request privacy permissions for a new build. '
          'Automatic deployment is also available through the CLI, without UI or SSH.',
        ),
        const SizedBox(height: 16),
        AnimatedBuilder(
          animation: widget.agent,
          builder: (_, _) {
            final tools = widget.agent.state['tools'] as Map? ?? const {};
            final jobs = (tools['jobs'] as List? ?? const [])
                .where((j) => j['kind'] == 'update')
                .toList();
            final caps = tools['capabilities'] as Map? ?? const {};
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '${caps.values.where((c) => c['dev_update'] == 1).length} Students ready for dev updates',
                ),
                for (final job in jobs.reversed.take(4)) ...[
                  const SizedBox(height: 14),
                  Text(
                    '${job['name']}',
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: 6),
                  for (final target in job['targets'] as List) ...[
                    Text(
                      '${target['id']} · ${target['status']}'
                      '${target['error'] == '' ? '' : ' · ${target['error']}'}'
                      '${target['reply'] == '' ? '' : ' · ${target['reply']}'}',
                    ),
                    if (![
                      'complete',
                      'error',
                      'cancelled',
                    ].contains(target['status']))
                      Padding(
                        padding: const EdgeInsets.symmetric(vertical: 6),
                        child: LinearProgressIndicator(
                          value: job['size'] == 0
                              ? null
                              : ((target['done'] as num) / (job['size'] as num))
                                    .clamp(0.0, 1.0),
                        ),
                      ),
                  ],
                ],
              ],
            );
          },
        ),
      ],
    ];
    return Dialog(
      child: SizedBox(
        width: 760,
        height: 570,
        child: Column(
          children: [
            const Padding(
              padding: EdgeInsets.all(20),
              child: Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  'Teacher settings',
                  style: TextStyle(fontSize: 22, fontWeight: FontWeight.w600),
                ),
              ),
            ),
            Expanded(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  SizedBox(
                    width: 152,
                    child: ListView(
                      children: [
                        for (var i = 0; i < sections.length; i++)
                          ListTile(
                            title: Text(sections[i]),
                            selected: page == i,
                            onTap: () => setState(() => page = i),
                          ),
                      ],
                    ),
                  ),
                  const VerticalDivider(width: 1),
                  Expanded(
                    child: SingleChildScrollView(
                      padding: const EdgeInsets.all(20),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: panels[page],
                      ),
                    ),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 12, 20, 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (error != null)
                    Text(
                      error!,
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.error,
                      ),
                    ),
                  const Text(
                    'Saving reconnects the room and closes current Observe sessions.',
                  ),
                  const SizedBox(height: 12),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      TextButton(
                        onPressed: () => Navigator.pop(context),
                        child: const Text('Cancel'),
                      ),
                      const SizedBox(width: 8),
                      FilledButton(
                        onPressed: save,
                        child: const Text('Save & reconnect'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
