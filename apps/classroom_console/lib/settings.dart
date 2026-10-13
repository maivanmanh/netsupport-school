import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'agent.dart';
import 'app_dialog.dart';

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
  ];
  int page = 0;
  String? error;
  bool preparingUpdate = false;
  bool preparingDownload = false;
  late final TextEditingController ip, port, adapter, maxFile, downloadPort;
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
    final shareNow = widget.agent.state['student_download'] as Map? ?? const {};
    final sharedPort = Uri.tryParse('${shareNow['url'] ?? ''}')?.port;
    downloadPort = TextEditingController(
      text: '${sharedPort ?? widget.agent.preferences['download_port'] ?? 80}',
    );
    maxFile = TextEditingController(
      text: '${section('tools')['max_file_mb'] ?? 300}',
    );
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
    maxFile.dispose();
    downloadPort.dispose();
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

  Future<void> startStudentDownload({bool choosePackage = false}) async {
    final number = int.tryParse(downloadPort.text.trim());
    if (number == null || number < 1 || number > 65535) {
      setState(() => error = 'Cổng tải xuống phải từ 1 đến 65535.');
      return;
    }
    setState(() {
      preparingDownload = true;
      error = null;
    });
    try {
      Map<String, dynamic>? picked;
      if (choosePackage) {
        picked = await const MethodChannel('classroom/files')
            .invokeMapMethod<String, dynamic>('chooseFile', {
              'directory': widget.agent.endpoint.parent.path,
            });
        if (picked == null) return;
      }
      await widget.agent.savePreferences({'download_port': number});
      await widget.agent.command('student_download_start', [], {
        if (picked != null) ...picked,
        'port': number,
      });
    } catch (e) {
      if (mounted)
        setState(() => error = 'Không chia sẻ được ứng dụng Student: $e');
    } finally {
      if (mounted) setState(() => preparingDownload = false);
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
        'tools': {
          'max_file_mb': limit,
          'connection_popup':
              (widget.agent.state['tools'] as Map?)?['connection_popup'] ??
              section('tools')['connection_popup'] ??
              false,
        },
      }),
    );
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
        choice('Độ phân giải tối đa', resolution, {
          1920: '1920 × 1080',
          1600: '1600 × 900',
          1280: '1280 × 720',
        }, (v) => resolution = v),
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
          'Tỉ lệ khung hình nguồn được giữ nguyên; màn hình nhỏ không bị phóng to. Con trỏ chuột của giảng viên luôn hiện với học viên.',
        ),
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
        field(
          downloadPort,
          'Cổng tải xuống HTTP',
          hint: '80: học viên chỉ cần nhập địa chỉ IP',
        ),
        note(
          'Có hiệu lực khi bạn bấm “Chia sẻ” bên dưới — không phải “Lưu & kết nối lại”.',
        ),
        AnimatedBuilder(
          animation: widget.agent,
          builder: (_, _) {
            final share =
                widget.agent.state['student_download'] as Map? ?? const {};
            final phase = share['phase'] ?? 'off';
            final active = phase != 'off';
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (phase == 'sharing') ...[
                  DialogPanel(
                    child: Padding(
                      padding: const EdgeInsets.all(14),
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
                          const SizedBox(height: 10),
                          AppDialog.secondary(
                            onPressed: () => Clipboard.setData(
                              ClipboardData(text: '${share['url']}'),
                            ),
                            icon: Icons.copy,
                            label: 'Sao chép địa chỉ',
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 12),
                ],
                if (phase == 'preparing') ...[
                  const LinearProgressIndicator(),
                  const SizedBox(height: 12),
                ],
                if (phase == 'error') ...[
                  InlineNotice('${share['error']}'),
                  const SizedBox(height: 12),
                ],
                if (active)
                  Align(
                    alignment: Alignment.centerLeft,
                    child: AppDialog.secondary(
                      onPressed: () async {
                        try {
                          await widget.agent.command('student_download_stop');
                        } catch (e) {
                          if (mounted) setState(() => error = '$e');
                        }
                      },
                      icon: Icons.stop,
                      label: 'Dừng chia sẻ ứng dụng',
                    ),
                  )
                else
                  Wrap(
                    spacing: 10,
                    runSpacing: 8,
                    children: [
                      AppDialog.primary(
                        onPressed: preparingDownload
                            ? null
                            : () => startStudentDownload(),
                        icon: Icons.download,
                        label: 'Chia sẻ ứng dụng Student đi kèm',
                      ),
                      AppDialog.secondary(
                        onPressed: preparingDownload
                            ? null
                            : () => startStudentDownload(choosePackage: true),
                        label: 'Chọn ZIP Student Windows…',
                      ),
                    ],
                  ),
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
          'Khay hệ thống của học viên Windows không có menu Thoát hay Khởi động lại. Task Manager và quyền quản trị vẫn dùng được. Chia sẻ HTTP dừng khi agent giảng viên thoát. Cổng 80 có thể cần quyền; dùng 8080 nếu không dùng được.',
        ),
      ],
    ];
    return AppDialog(
      icon: Icons.settings_outlined,
      title: 'Cài đặt giảng viên',
      subtitle: 'Kết nối, phát màn hình, quan sát và công cụ quản trị.',
      width: 860,
      height: 520,
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
        AppDialog.primary(
          label: 'Lưu & kết nối lại',
          icon: Icons.save_outlined,
          onPressed: save,
        ),
      ],
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            width: 208,
            color: const Color(0xfff6f8fa),
            child: ListView(
              key: const ValueKey('teacher-settings-sections'),
              padding: const EdgeInsets.all(10),
              children: [
                for (var i = 0; i < sections.length; i++)
                  Padding(
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
                              Icon(
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
