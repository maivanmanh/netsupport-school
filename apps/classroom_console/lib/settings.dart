import 'icon_tints.dart';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'settings_param_view.dart';
import 'settings_params.dart';

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
    ('Kết nối', Icons.lan_outlined),
    ('Phát màn hình', Icons.cast),
    ('Quan sát', Icons.visibility_outlined),
    ('Truyền tệp', Icons.folder_shared_outlined),
    ('Chẩn đoán', Icons.bug_report_outlined),
    ('Cập nhật học viên', Icons.system_update_alt),
    ('Cài đặt máy học viên', Icons.download_for_offline_outlined),
    ('Danh sách & giám sát', Icons.view_module_outlined),
    ('Theo dõi hoạt động', Icons.monitor_heart_outlined),
    ('Quy tắc lớp', Icons.rule),
    ('Ghi màn hình', Icons.video_camera_back_outlined),
    ('Thông báo', Icons.notifications_outlined),
  ];

  /// How the pages are grouped in the left list: heading, then page numbers.
  static const groups = [
    ('Lớp học', [0, 1, 2]),
    ('Máy học viên', [7, 8, 9, 6]),
    ('Lưu & truyền', [10, 3]),
    ('Hệ thống', [11, 4, 5]),
  ];
  static const _presets = [
    (
      id: 'wired',
      label: 'Phòng máy dây (mạng tốt)',
      width: 1920,
      fps: 30,
      mbps: 6.0,
      mode: 'normal',
    ),
    (
      id: 'wifi',
      label: 'Có Wi-Fi',
      width: 1600,
      fps: 24,
      mbps: 4.0,
      mode: 'economy',
    ),
    (
      id: 'weak',
      label: 'Máy yếu / mạng yếu',
      width: 1280,
      fps: 20,
      mbps: 2.5,
      mode: 'minimal',
    ),
  ];
  int page = 0;
  String? error;
  bool preparingUpdate = false;

  /// The Student ZIP is shared on this port, always. No field: nothing to choose.
  static const sharePort = 8080;
  bool shareBusy = false;
  String? shareError;
  String? scriptSaved;
  late final TextEditingController ip, port, adapter, maxFile, bandwidthCap;
  late final ParamStore params;
  String perfMode = 'normal';
  bool autoWifi = false;
  int resolution = 1920, fps = 30, observeFps = 60, interval = 2000;
  double bitrate = 6;
  bool adaptive = true, enhanced = true;
  bool presentationPreserveAspect = false;
  bool autoCollect = false;
  String codec = 'auto', level = 'info';
  Map get effective => widget.agent.state['settings'] as Map? ?? const {};
  Map section(String key) => effective[key] as Map? ?? const {};

  @override
  void initState() {
    super.initState();
    maxFile = TextEditingController(
      text: '${section('tools')['max_file_mb'] ?? 300}',
    );
    params = ParamStore(
      settings: Map<String, dynamic>.from(effective),
      preferences: widget.agent.preferences,
    );
    final performance = section('performance');
    perfMode = const {'economy', 'minimal'}.contains(performance['mode'])
        ? '${performance['mode']}'
        : 'normal';
    autoWifi = performance['auto_wifi'] == true;
    final cap = (performance['bandwidth_cap_mbps'] as num?)?.toInt() ?? 0;
    bandwidthCap = TextEditingController(text: cap == 0 ? '' : '$cap');
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
    presentationPreserveAspect =
        video['presentation_preserve_aspect'] as bool? ?? false;
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
    savedSnapshot = snapshot();
  }

  /// Every value that "Lưu & kết nối lại" would write, as one comparable string.
  /// (The Student share port is not saved here, so it is not part of it.)
  String snapshot() => [
    ip.text.trim(),
    port.text.trim(),
    adapter.text.trim(),
    maxFile.text.trim(),
    resolution,
    fps,
    observeFps,
    interval,
    bitrate,
    adaptive,
    enhanced,
    presentationPreserveAspect,
    autoCollect,
    codec,
    level,
    perfMode,
    autoWifi,
    bandwidthCap.text.trim(),
    // The popup switch acts immediately but is only remembered by Save.
    (widget.agent.state['tools'] as Map?)?['connection_popup'] ?? false,
  ].join('\u0001');
  late final String savedSnapshot;
  bool get legacyDirty => snapshot() != savedSnapshot;
  bool get dirty => legacyDirty || params.dirty;
  bool get needsRestart => legacyDirty || params.needsRestart;

  @override
  void dispose() {
    ip.dispose();
    port.dispose();
    adapter.dispose();
    maxFile.dispose();
    params.dispose();
    bandwidthCap.dispose();
    super.dispose();
  }

  Widget field(
    TextEditingController controller,
    String label, {
    String? hint,
    bool enabled = true,
  }) => Padding(
    padding: const EdgeInsets.only(bottom: 14),
    child: TextField(
      controller: controller,
      enabled: enabled,
      decoration: InputDecoration(labelText: label, hintText: hint),
    ),
  );

  Widget choice<T>(
    String label,
    T value,
    Map<T, String> options,
    void Function(T) change,
  ) => Padding(
    padding: const EdgeInsets.only(bottom: 14),
    child: DropdownButtonFormField<T>(
      initialValue: value,
      isExpanded: true,
      decoration: InputDecoration(labelText: label),
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
      if (mounted) setState(() => error = 'Không mở được thư mục nhật ký: $e');
    }
  }

  /// A copy of the installer script on the Desktop, for handing out through
  /// other tools. It carries this Teacher's address as of when sharing started;
  /// the copy served at the link always uses the address the Student reaches.
  Future<void> saveInstallScript(Map share) async {
    final source = '${share['script_path'] ?? ''}';
    try {
      final home =
          Platform.environment['USERPROFILE'] ??
          Platform.environment['HOME'] ??
          '';
      final sep = Platform.pathSeparator;
      final target = '$home${sep}Desktop${sep}Install-Classroom-Student.bat';
      await File(source).copy(target);
      if (mounted) {
        setState(
          () => scriptSaved =
              'Đã lưu: $target\nTệp ghi địa chỉ ${share['url']}. Nếu IP máy giảng viên đổi, tải lại tệp từ liên kết hoặc chạy với tham số /teacher <IP mới>.',
        );
      }
    } catch (e) {
      if (mounted) setState(() => scriptSaved = 'Không lưu được tệp: $e');
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
      if (mounted) setState(() => error = 'Không bắt đầu được cập nhật: $e');
    } finally {
      if (mounted) setState(() => preparingUpdate = false);
    }
  }

  /// Switch on: serve the bundled Student ZIP. Failures (port busy, firewall)
  /// are reported next to the switch, which stays off.
  Future<void> setShare(bool on) async {
    setState(() {
      shareBusy = true;
      shareError = null;
    });
    try {
      await widget.agent.command(
        on ? 'student_download_start' : 'student_download_stop',
        [],
        on ? {'port': sharePort} : null,
      );
    } catch (e) {
      if (mounted) {
        setState(
          () => shareError = on
              ? 'Không bật được chia sẻ trên cổng $sharePort. Cổng có thể đang được dùng hoặc bị tường lửa chặn. ($e)'
              : '$e',
        );
      }
    } finally {
      if (mounted) setState(() => shareBusy = false);
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
        error = 'Nhập địa chỉ IPv4 (hoặc để trống) và cổng từ 1024 đến 65524.';
      });
      return;
    }
    final limit = int.tryParse(maxFile.text.trim());
    if (limit == null || limit < 1 || limit > 4096) {
      setState(() {
        page = 3;
        error = 'Giới hạn tệp phải từ 1 đến 4096 MB.';
      });
      return;
    }
    final problem = params.validate();
    if (problem != null) {
      setState(() {
        page = problem.page;
        error = problem.message;
      });
      return;
    }
    final capText = bandwidthCap.text.trim();
    final cap = capText.isEmpty ? 0 : int.tryParse(capText);
    if (cap == null || (cap != 0 && (cap < 2 || cap > 10000))) {
      setState(() {
        page = 1;
        error =
            'Giới hạn băng thông phải từ 2 đến 10000 Mbps (để trống nếu không giới hạn).';
      });
      return;
    }
    final overlay = <String, dynamic>{
      'performance': {
        'mode': perfMode,
        'auto_wifi': autoWifi,
        'bandwidth_cap_mbps': cap,
      },
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
        'presentation_preserve_aspect': presentationPreserveAspect,
      },
      'monitoring': {...section('monitoring'), 'interval_ms': interval},
      'observe': {
        ...section('observe'),
        'enhanced': enhanced,
        'codec': codec,
        'max_fps': observeFps,
      },
      'diagnostics': {'auto_collect': autoCollect},
      'tools': {
        'max_file_mb': limit,
        'connection_popup':
            (widget.agent.state['tools'] as Map?)?['connection_popup'] ??
            section('tools')['connection_popup'] ??
            false,
      },
    };
    params.configSections().forEach((section, values) {
      overlay[section] = {...(overlay[section] as Map? ?? const {}), ...values};
    });
    final prefs = params.changedPreferences();
    if (prefs.isNotEmpty) widget.agent.savePreferences(prefs);
    if (!needsRestart) {
      applyWithoutRestart(overlay);
      return;
    }
    Navigator.pop(
      context,
      TeacherSettings(address.isEmpty ? 'auto' : address, n, overlay),
    );
  }

  /// Settings that the running Teacher can take: stored and used at once, so
  /// the connection to the Students is never interrupted.
  Future<void> applyWithoutRestart(Map<String, dynamic> overlay) async {
    try {
      if (widget.agent.connected) await widget.agent.applySettings(overlay);
      if (!mounted) return;
      final messenger = ScaffoldMessenger.maybeOf(context);
      Navigator.pop(context);
      messenger?.showSnackBar(
        const SnackBar(
          content: Text('Đã áp dụng cài đặt — không cần kết nối lại.'),
        ),
      );
    } catch (e) {
      if (mounted) setState(() => error = 'Không áp dụng được cài đặt: $e');
    }
  }

  Widget note(String text) => Padding(
    padding: const EdgeInsets.only(bottom: 14),
    child: Text(
      text,
      style: const TextStyle(
        fontSize: 12.5,
        height: 1.45,
        color: AppDialog.muted,
      ),
    ),
  );

  /// Kbps each Student gets from a class-wide cap, as the Teacher computes it.
  static int perStudentKbps(int capMbps, int students) {
    final each = capMbps * 1000 * 75 ~/ 100 ~/ (students < 1 ? 1 : students);
    return each < 300 ? 300 : each;
  }

  /// A cap so low that every Student would get less than the 300 kbps floor.
  static bool capUnreachable(int capMbps, int students) =>
      capMbps * 1000 * 75 ~/ 100 ~/ (students < 1 ? 1 : students) < 300;

  /// How the picture shrinks to fit a share (mirrors `Config::for_stream`).
  static String capPictureNote(int kbps) => kbps >= 4000
      ? ''
      : kbps >= 2000
      ? ', hình tối đa 1280 × 720'
      : kbps >= 900
      ? ', hình tối đa 960 × 540'
      : ', hình tối đa 640 × 360';

  static const _modeNotes = {
    'normal': 'Giữ đúng các giá trị bên dưới.',
    'economy':
        'Tối đa 1280 × 720, 20 fps, khoảng 3 Mbps. Quan sát một máy dùng cấu hình lớp học (không dùng chế độ 60 fps), ảnh thu nhỏ làm mới 4 giây một lần với chất lượng thấp hơn. Hợp với Wi-Fi hoặc máy học viên yếu.',
    'minimal':
        'Tối đa 960 × 540, 12 fps, khoảng 1,2 Mbps; ảnh thu nhỏ 6 giây một lần. Dùng cho máy rất yếu hoặc mạng rất chậm; chữ nhỏ có thể khó đọc.',
  };

  /// What the Teacher is actually doing right now (published by the agent).
  String? performanceInForce() {
    final p = widget.agent.state['performance'] as Map?;
    if (p == null || p['active'] == null || p['active'] == 'normal') {
      return null;
    }
    final name = p['active'] == 'minimal' ? 'Rất tiết kiệm' : 'Tiết kiệm';
    final why = p['reason'] == 'wifi' ? ' (do đang dùng Wi-Fi)' : '';
    final mbps = ((p['bitrate_kbps'] as num? ?? 0) / 1000).toStringAsFixed(1);
    return 'Đang áp dụng: $name$why · ${p['width']} × ${p['height']} · ${p['max_fps']} fps · $mbps Mbps';
  }

  /// One-click starting points; every value can still be changed afterwards.
  Widget _presetCard() => DialogSection(
    title: 'Hồ sơ nhanh',
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            for (final preset in _presets)
              OutlinedButton(
                key: ValueKey('preset-${preset.id}'),
                onPressed: () => setState(() {
                  resolution = preset.width;
                  fps = preset.fps;
                  bitrate = preset.mbps;
                  perfMode = preset.mode;
                  autoWifi = false;
                }),
                child: Text(preset.label),
              ),
          ],
        ),
        const SizedBox(height: 8),
        const Text(
          'Chọn phù hợp phòng máy rồi bấm Lưu. Bạn vẫn chỉnh được từng giá trị bên dưới.',
          style: TextStyle(fontSize: 12.5, height: 1.4, color: AppDialog.muted),
        ),
      ],
    ),
  );

  Widget _performanceCard() => DialogSection(
    title: 'Chế độ tiết kiệm (mạng yếu / máy yếu)',
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SegmentedButton<String>(
          showSelectedIcon: false,
          segments: const [
            ButtonSegment(value: 'normal', label: Text('Bình thường')),
            ButtonSegment(value: 'economy', label: Text('Tiết kiệm')),
            ButtonSegment(value: 'minimal', label: Text('Rất tiết kiệm')),
          ],
          selected: {perfMode},
          onSelectionChanged: (v) => setState(() => perfMode = v.first),
        ),
        const SizedBox(height: 8),
        Text(
          _modeNotes[perfMode]!,
          style: const TextStyle(
            fontSize: 12.5,
            height: 1.4,
            color: AppDialog.muted,
          ),
        ),
        if (perfMode == 'normal') ...[
          switchRow(
            'Tự chuyển sang Tiết kiệm khi máy giảng viên dùng Wi-Fi',
            autoWifi,
            (v) => setState(() => autoWifi = v),
          ),
        ],
        const SizedBox(height: 4),
        ListenableBuilder(
          listenable: Listenable.merge([bandwidthCap, widget.agent]),
          builder: (context, _) {
            final cap = int.tryParse(bandwidthCap.text.trim()) ?? 0;
            final online = widget.agent.students
                .where((s) => s['online'] == true)
                .length;
            return TextField(
              controller: bandwidthCap,
              keyboardType: TextInputType.number,
              decoration: InputDecoration(
                labelText: 'Giới hạn băng thông gửi cho cả lớp (Mbps)',
                hintText: 'Để trống = không giới hạn',
                helperText: cap >= 2
                    ? (capUnreachable(cap, online)
                          ? 'Quá thấp cho $online máy: mỗi máy cần tối thiểu 300 kbps, nên băng thông thực tế sẽ cao hơn mức này.'
                          : 'Với $online máy đang online: khoảng ${perStudentKbps(cap, online)} kbps mỗi máy${capPictureNote(perStudentKbps(cap, online))}.')
                    : null,
              ),
            );
          },
        ),
        if (performanceInForce() case final inForce?) ...[
          const SizedBox(height: 10),
          InlineNotice(inForce, tone: AppTone.info),
        ],
        const SizedBox(height: 6),
        const Text(
          'Các giá trị chỉ bị hạ xuống, không bao giờ tăng lên. Có hiệu lực sau khi “Lưu & kết nối lại”.',
          style: TextStyle(fontSize: 12, color: AppDialog.muted),
        ),
      ],
    ),
  );

  Widget switchRow(
    String title,
    bool value,
    ValueChanged<bool>? onChanged, {
    String? subtitle,
  }) => SwitchListTile(
    contentPadding: EdgeInsets.zero,
    dense: true,
    title: Text(title),
    subtitle: subtitle == null ? null : Text(subtitle),
    value: value,
    onChanged: onChanged,
  );

  Widget pageHeader(String title, String description) => Padding(
    padding: const EdgeInsets.only(bottom: 16),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          title,
          style: const TextStyle(
            fontSize: 16,
            fontWeight: FontWeight.w700,
            color: AppDialog.ink,
          ),
        ),
        const SizedBox(height: 4),
        Text(
          description,
          style: const TextStyle(
            fontSize: 12.5,
            height: 1.45,
            color: AppDialog.muted,
          ),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    final launchedWithAddress =
        widget.agent.option('--bind-ip') != null ||
        widget.agent.option('--control-port') != null;
    final panels = <List<Widget>>[
      [
        pageHeader(
          'Kết nối',
          'Học viên kết nối tới máy này. Dùng cùng một cổng điều khiển trên mọi máy.',
        ),
        field(
          ip,
          'Địa chỉ IPv4 của giảng viên',
          hint: 'Tự động',
          enabled: widget.agent.option('--bind-ip') == null,
        ),
        field(
          port,
          'Cổng điều khiển',
          enabled: widget.agent.option('--control-port') == null,
        ),
        field(
          adapter,
          'Tên card mạng',
          hint: 'Tự động: Ethernet đang hoạt động / tuyến mặc định',
        ),
        InlineNotice(
          'Địa chỉ hiện tại: ${widget.agent.state['teacher_ip'] ?? 'đang kết nối'}',
          tone: AppTone.info,
        ),
        const SizedBox(height: 12),
        note(
          'Chế độ tự động bỏ qua các card mạng ảo không hoạt động. IP nhập tay phải thuộc về máy này.',
        ),
        if (launchedWithAddress)
          note(
            'IP / cổng truyền khi khởi chạy được ưu tiên hơn cài đặt đã lưu.',
          ),
      ],
      [
        pageHeader(
          'Phát màn hình',
          'Giữ chữ dễ đọc và khung hình mới. Chế độ chất lượng tự động điều chỉnh bitrate và tốc độ khung hình khi mạng bị quá tải.',
        ),
        _presetCard(),
        const SizedBox(height: 12),
        _performanceCard(),
        const SizedBox(height: 12),
        choice('Độ phân giải tối đa', resolution, {
          1920: '1920 × 1080',
          1600: '1600 × 900',
          1280: '1280 × 720',
        }, (v) => resolution = v),
        choice(
          'Hiển thị trên màn hình học viên',
          presentationPreserveAspect,
          {
            false: 'Kéo giãn phủ kín màn hình (mặc định)',
            true: 'Giữ tỉ lệ nguồn (có thể có viền đen)',
          },
          (v) => presentationPreserveAspect = v,
        ),
        note(
          'Kéo giãn phủ kín màn hình học viên, chấp nhận hình bị méo khi khác tỉ lệ. Lựa chọn áp dụng cho lần chiếu tiếp theo sau khi lưu.',
        ),
        choice('Tốc độ khung hình tối đa', fps, {
          15: '15 fps',
          20: '20 fps',
          24: '24 fps',
          30: '30 fps',
        }, (v) => fps = v),
        DialogSection(
          title: 'Bitrate mục tiêu: ${bitrate.toStringAsFixed(1)} Mbps',
          child: Slider(
            value: bitrate,
            min: 1.5,
            max: 8,
            divisions: 13,
            label: '${bitrate.toStringAsFixed(1)} Mbps',
            onChanged: (v) => setState(() => bitrate = v),
          ),
        ),
        switchRow(
          'Chất lượng tự động',
          adaptive,
          (v) => setState(() => adaptive = v),
        ),
        const SizedBox(height: 8),
        note(
          'Độ phân giải truyền giữ tỉ lệ nguồn; học viên hiển thị theo chế độ đã chọn. Con trỏ chuột của giảng viên luôn hiện với học viên.',
        ),
        const SizedBox(height: 8),
        ParamPageView(store: params, page: pageStream),
      ],
      [
        pageHeader(
          'Quan sát',
          'Cài đặt khi xem hoặc điều khiển màn hình của một học viên.',
        ),
        switchRow(
          'Quan sát nâng cao một-một',
          enhanced,
          (v) => setState(() => enhanced = v),
          subtitle: 'Dùng các tính năng phần cứng mà cả hai máy cùng hỗ trợ.',
        ),
        const SizedBox(height: 8),
        choice('Codec ưu tiên', codec, {
          'auto': 'Tự động',
          'h264': 'H.264',
          'h265': 'H.265 khi được hỗ trợ',
        }, (v) => codec = v),
        choice('Trần tốc độ khung hình khi quan sát', observeFps, {
          30: '30 fps',
          60: '60 fps khi được hỗ trợ',
        }, (v) => observeFps = v),
        choice('Làm mới ảnh thu nhỏ trong danh sách', interval, {
          for (var i = 1; i <= 10; i++) i * 1000: '$i giây',
        }, (v) => interval = v),
        note(
          'Cấu hình không được hỗ trợ sẽ chuyển về H.264. Mỗi cửa sổ Quan sát mở ở chế độ chỉ xem; Điều khiển là thao tác chủ động.',
        ),
      ],
      [
        pageHeader(
          'Truyền tệp',
          'Kéo thả tệp hoặc thư mục vào cửa sổ Quan sát/Điều khiển. Thư mục được gửi dưới dạng ZIP; học viên nhận trên Desktop.',
        ),
        field(maxFile, 'Dung lượng tệp tối đa (MB)', hint: '300'),
        note(
          'Áp dụng cho mỗi tệp hoặc mỗi ZIP hoàn chỉnh. Phạm vi: 1–4096 MB.',
        ),
        const SizedBox(height: 8),
        ParamPageView(store: params, page: pageTransfer),
      ],
      [
        pageHeader(
          'Chẩn đoán',
          'Thu nhật ký của giảng viên và học viên để tìm lỗi. Không gồm ảnh chụp màn hình, nội dung tin nhắn/tệp và token IPC.',
        ),
        switchRow(
          'Tự thu thập sau khi dừng chiếu',
          autoCollect,
          (v) => setState(() => autoCollect = v),
        ),
        const SizedBox(height: 8),
        Align(
          alignment: Alignment.centerLeft,
          child: AppDialog.primary(
            icon: Icons.archive_outlined,
            label: widget.selected.isEmpty
                ? 'Thu chẩn đoán · cả lớp'
                : 'Thu chẩn đoán · ${widget.selected.length} máy đã chọn',
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
                    ? 'Đã thu ${status['collected']}/${status['total']} học viên; ${status['missing']} máy không khả dụng.\n${status['path']}\n${(status['students'] as List? ?? []).where((s) => s['status'] != 'collected').map((s) => "${s['name']} (${s['ip']}): ${s['reason']}").join('\n')}'
                    : status['phase'] == 'waiting_for_stop'
                    ? 'Đang xếp hàng: việc thu thập bắt đầu sau khi dừng chiếu.'
                    : status['phase'] == 'error'
                    ? 'Thu thập thất bại: ${status['error']}'
                    : status['phase'] == 'collecting'
                    ? 'Đang thu: ${status['completed'] ?? 0}/${status['total']} học viên'
                    : 'Sẵn sàng thu nhật ký của giảng viên và học viên.',
                style: const TextStyle(fontSize: 12.5, height: 1.45),
              ),
            );
          },
        ),
        Align(
          alignment: Alignment.centerLeft,
          child: AppDialog.secondary(
            onPressed: () => showLogs(diagnostics: true),
            icon: Icons.folder_zip_outlined,
            label: 'Mở thư mục chẩn đoán',
          ),
        ),
        const SizedBox(height: 10),
        note(
          'Việc thu thập chờ tới khi dừng chiếu, dùng kết nối riêng giới hạn 512 KiB/s và tạm dừng khi bắt đầu phiên chiếu mới. Mỗi ZIP gồm bản build, cấu hình và định danh máy.',
        ),
        const Divider(height: 28),
        choice('Mức chi tiết nhật ký', level, {
          'info': 'Bình thường',
          'debug': 'Chi tiết',
          'warn': 'Chỉ cảnh báo và lỗi',
        }, (v) => level = v),
        Wrap(
          spacing: 10,
          runSpacing: 8,
          children: [
            AppDialog.secondary(
              onPressed: showLogs,
              icon: Icons.folder_open,
              label: 'Mở thư mục nhật ký',
            ),
            if (Platform.isMacOS)
              AppDialog.secondary(
                onPressed: () async {
                  await const MethodChannel(
                    'classroom/permissions',
                  ).invokeMethod<void>('openScreenRecordingSettings');
                },
                icon: Icons.privacy_tip_outlined,
                label: 'Quyền Screen Recording',
              ),
          ],
        ),
        const SizedBox(height: 14),
        note(
          'Giảng viên · ${Platform.operatingSystem}. Đóng cửa sổ chỉ thu vào khay hệ thống; “Thoát Classroom” đóng mọi cửa sổ và tiến trình media.',
        ),
      ],
      [
        pageHeader(
          'Cập nhật học viên (phát triển)',
          'Chọn gói ZIP tạo bằng classroom-dev-update.py. Học viên tự nhận, kiểm tra, khởi động lại và kết nối lại. Hãy dừng chiếu và đóng Quan sát trước. Cấu hình và nhật ký được giữ nguyên.',
        ),
        Align(
          alignment: Alignment.centerLeft,
          child: AppDialog.primary(
            onPressed: preparingUpdate ? null : updateStudents,
            icon: Icons.system_update_alt,
            busy: preparingUpdate,
            label: preparingUpdate
                ? 'Đang chuẩn bị gói…'
                : widget.selected.isEmpty
                ? 'Cập nhật các máy tương thích…'
                : 'Cập nhật ${widget.selected.length} máy đã chọn…',
          ),
        ),
        const SizedBox(height: 14),
        note(
          'Học viên có bộ cập nhật phải được cài thủ công một lần. macOS có thể yêu cầu cấp lại quyền riêng tư cho bản build mới. Cũng có thể triển khai tự động bằng CLI, không cần giao diện hay SSH.',
        ),
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
                InlineNotice(
                  '${caps.values.where((c) => c['dev_update'] == 1).length} học viên sẵn sàng nhận cập nhật',
                  tone: AppTone.info,
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
                      style: const TextStyle(fontSize: 12.5),
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
      [
        pageHeader(
          'Cài đặt máy học viên',
          'Học viên mở địa chỉ của giảng viên trong Chrome, tải ZIP, giải nén toàn bộ vào một thư mục cục bộ và bấm đúp ClassroomStudent.exe. Gói ZIP đã gồm địa chỉ của phòng này.',
        ),
        AnimatedBuilder(
          animation: widget.agent,
          builder: (_, _) {
            final share =
                widget.agent.state['student_download'] as Map? ?? const {};
            final phase = '${share['phase'] ?? 'off'}';
            final on = phase == 'preparing' || phase == 'sharing';
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                switchRow(
                  'Chia sẻ ứng dụng Student đi kèm',
                  on,
                  shareBusy ? null : setShare,
                  subtitle: on
                      ? 'Đang bật: học viên mở địa chỉ bên dưới để tải ứng dụng.'
                      : 'Đang tắt. Bật để học viên tải ứng dụng Student từ máy giảng viên.',
                ),
                if (phase == 'sharing') ...[
                  const SizedBox(height: 8),
                  DialogPanel(
                    child: Padding(
                      padding: const EdgeInsets.all(14),
                      child: Row(
                        children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                SelectableText(
                                  '${share['url']}',
                                  style: const TextStyle(
                                    fontSize: 18,
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                                const SizedBox(height: 4),
                                Text(
                                  '${share['arch']} · ${((share['size'] as num? ?? 0) / 1048576).toStringAsFixed(1)} MB · ${share['downloads'] ?? 0} lượt tải',
                                  style: const TextStyle(
                                    fontSize: 12.5,
                                    color: AppDialog.muted,
                                  ),
                                ),
                              ],
                            ),
                          ),
                          const SizedBox(width: 12),
                          AppDialog.secondary(
                            onPressed: () => Clipboard.setData(
                              ClipboardData(text: '${share['url']}'),
                            ),
                            icon: Icons.copy,
                            label: 'Sao chép',
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 10),
                  DialogPanel(
                    child: Padding(
                      padding: const EdgeInsets.all(14),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          const Text(
                            'Cài đặt một bước (tệp .bat)',
                            style: TextStyle(fontWeight: FontWeight.w700),
                          ),
                          const SizedBox(height: 4),
                          const Text(
                            'Học viên tải tệp này, bấm chuột phải → Run as administrator: máy tự tải gói về, giải nén vào Program Files\\Classroom và chạy. Nội dung tự lấy địa chỉ IP hiện tại của máy giảng viên. Phần mềm điều khiển cũ có thể chép tệp xuống máy học viên và chạy với tham số /quiet.',
                            style: TextStyle(
                              fontSize: 12.5,
                              height: 1.4,
                              color: AppDialog.muted,
                            ),
                          ),
                          const SizedBox(height: 8),
                          SelectableText(
                            '${share['script_url']}',
                            key: const ValueKey('script-url'),
                            style: const TextStyle(
                              fontSize: 15,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 8),
                          Wrap(
                            spacing: 8,
                            runSpacing: 8,
                            children: [
                              AppDialog.secondary(
                                onPressed: () => Clipboard.setData(
                                  ClipboardData(text: '${share['script_url']}'),
                                ),
                                icon: Icons.copy,
                                label: 'Sao chép liên kết',
                              ),
                              KeyedSubtree(
                                key: const ValueKey('script-save'),
                                child: AppDialog.secondary(
                                  onPressed: () => saveInstallScript(share),
                                  icon: Icons.save_alt,
                                  label: 'Lưu tệp .bat ra Desktop',
                                ),
                              ),
                            ],
                          ),
                          if (scriptSaved != null) ...[
                            const SizedBox(height: 8),
                            SelectableText(
                              scriptSaved!,
                              style: const TextStyle(fontSize: 12.5),
                            ),
                          ],
                        ],
                      ),
                    ),
                  ),
                ],
                if (phase == 'error') ...[
                  const SizedBox(height: 8),
                  InlineNotice(
                    'Chia sẻ gặp lỗi: ${share['error']}. Hãy tắt rồi bật lại công tắc.',
                  ),
                ],
                if (shareError != null) ...[
                  const SizedBox(height: 8),
                  InlineNotice(shareError!),
                ],
              ],
            );
          },
        ),
        const SizedBox(height: 18),
        AnimatedBuilder(
          animation: widget.agent,
          builder: (_, _) {
            final tools = widget.agent.state['tools'] as Map? ?? const {};
            return switchRow(
              'Cửa sổ trạng thái kết nối trên máy học viên',
              tools['connection_popup'] as bool? ?? false,
              (value) async {
                try {
                  await widget.agent.command('connection_popup', [], {
                    'enabled': value,
                  });
                } catch (e) {
                  if (mounted) setState(() => error = '$e');
                }
              },
              subtitle:
                  'Hiện trạng thái kết nối; tự ẩn sau 3 giây khi đã kết nối. Áp dụng ngay cho học viên hỗ trợ; “Lưu” sẽ ghi nhớ cài đặt này.',
            );
          },
        ),
        const SizedBox(height: 10),
        note(
          'Khay hệ thống của học viên Windows không có menu Thoát hay Khởi động lại. Task Manager và quyền quản trị vẫn dùng được. Chia sẻ (cổng 8080) mặc định tắt và tự dừng khi chương trình giảng viên thoát.',
        ),
      ],
      [
        pageHeader(
          'Danh sách & giám sát',
          'Cách các máy học viên hiện trên màn hình chính. Mỗi thay đổi áp dụng ngay khi bấm Lưu, không cần kết nối lại.',
        ),
        ParamPageView(store: params, page: pageRoster),
      ],
      [
        pageHeader(
          'Theo dõi hoạt động',
          'Thông tin về ứng dụng, trang web, clipboard và tệp tải xuống khi bật nút "Theo dõi hoạt động". Học viên áp dụng ngay khi giảng viên bấm Lưu.',
        ),
        ParamPageView(store: params, page: pageActivity),
      ],
      [
        pageHeader(
          'Quy tắc lớp',
          'Các thao tác quản lý lớp như tắt máy, khóa màn hình.',
        ),
        ParamPageView(store: params, page: pageRules),
      ],
      [
        pageHeader(
          'Ghi màn hình',
          'Chất lượng, nơi lưu và các giới hạn an toàn của tính năng ghi màn hình học viên. Áp dụng cho phiên ghi bắt đầu sau khi lưu; phiên đang ghi giữ cài đặt cũ. Máy học viên cần bản cập nhật mới để nhận chất lượng riêng.',
        ),
        ParamPageView(store: params, page: pageRecording),
      ],
      [
        pageHeader(
          'Thông báo',
          'Thời gian hiện các thông báo nhỏ trên màn hình chính.',
        ),
        ParamPageView(store: params, page: pageNotices),
      ],
    ];
    return AppDialog(
      icon: Icons.settings_outlined,
      title: 'Cài đặt giảng viên',
      subtitle: 'Kết nối, phát màn hình, quan sát và công cụ quản trị.',
      width: 980,
      height: 560,
      scrollable: false,
      bodyPadding: EdgeInsets.zero,
      footerLeading: Text(
        error ??
            'Lưu sẽ kết nối lại phòng học và đóng các phiên Quan sát hiện tại.',
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(
          fontSize: 12,
          height: 1.35,
          color: error == null
              ? AppDialog.muted
              : Theme.of(context).colorScheme.error,
          fontWeight: error == null ? FontWeight.w400 : FontWeight.w600,
        ),
      ),
      actions: [
        AppDialog.cancel(context),
        ListenableBuilder(
          // Text fields and the popup switch change without a setState here.
          listenable: Listenable.merge([
            ip,
            port,
            adapter,
            maxFile,
            params,
            bandwidthCap,
            widget.agent,
          ]),
          builder: (context, _) => AppDialog.primary(
            label: needsRestart ? 'Lưu & kết nối lại' : 'Lưu & áp dụng ngay',
            icon: Icons.save_outlined,
            onPressed: dirty ? save : null,
          ),
        ),
      ],
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            width: 252,
            color: const Color(0xfff6f8fa),
            child: ListView(
              key: const ValueKey('teacher-settings-sections'),
              padding: const EdgeInsets.all(10),
              children: [
                for (final group in groups) ...[
                  Padding(
                    padding: const EdgeInsets.fromLTRB(12, 10, 12, 4),
                    child: Text(
                      group.$1.toUpperCase(),
                      style: const TextStyle(
                        fontSize: 10.5,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.6,
                        color: AppDialog.muted,
                      ),
                    ),
                  ),
                  for (final i in group.$2)
                    Padding(
                      key: ValueKey('settings-page-$i'),
                      padding: const EdgeInsets.only(bottom: 2),
                      child: Material(
                        color: page == i
                            ? const Color(0xffdcecef)
                            : Colors.transparent,
                        borderRadius: BorderRadius.circular(8),
                        child: InkWell(
                          borderRadius: BorderRadius.circular(8),
                          onTap: () => setState(() => page = i),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 12,
                              vertical: 10,
                            ),
                            child: Row(
                              children: [
                                TIcon(
                                  sections[i].$2,
                                  size: 18,
                                  color: page == i
                                      ? const Color(0xff156b79)
                                      : AppDialog.muted,
                                ),
                                const SizedBox(width: 10),
                                Expanded(
                                  child: Text(
                                    sections[i].$1,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                      fontSize: 13,
                                      fontWeight: page == i
                                          ? FontWeight.w700
                                          : FontWeight.w500,
                                      color: page == i
                                          ? const Color(0xff156b79)
                                          : AppDialog.ink,
                                    ),
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ],
            ),
          ),
          const VerticalDivider(width: 1, color: AppDialog.border),
          Expanded(
            child: SingleChildScrollView(
              padding: const EdgeInsets.fromLTRB(26, 22, 26, 22),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: panels[page],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
