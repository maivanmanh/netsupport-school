import 'package:flutter/material.dart';

/// One adjustable setting. The list below is the single description of what
/// the Settings screen can change: its label, unit, range, default (always how
/// the app behaved before it was adjustable) and where it is stored.
///
/// Ids are `section.key` for the Teacher's configuration (validated again by
/// the Rust side) and `pref:key` for console preferences (look and feel).
enum ParamKind { integer, choice, toggle, text }

class Param {
  const Param(
    this.id,
    this.label, {
    required this.kind,
    required this.page,
    required this.def,
    this.unit = '',
    this.help = '',
    this.min,
    this.max,
    this.options,
    this.advanced = false,
    this.hot = true,
    this.hint,
  });

  final String id, label, unit, help;
  final ParamKind kind;

  /// Index of the Settings page that shows it (for jumping to a problem).
  final int page;
  final Object def;
  final int? min, max;
  final Map<Object, String>? options;

  /// Shown under a collapsed "Nâng cao" heading.
  final bool advanced;

  /// Takes effect without restarting the Teacher.
  final bool hot;
  final String? hint;

  bool get isPref => id.startsWith('pref:');
  String get section => id.split('.').first;
  String get key =>
      isPref ? id.substring(5) : id.substring(id.indexOf('.') + 1);
}

// Settings pages that hold parameters.
const pageStream = 1;
const pageTransfer = 3;
const pageRoster = 7;
const pageActivity = 8;
const pageRules = 9;
const pageRecording = 10;
const pageNotices = 11;

const kParams = <Param>[
  // ---- Danh sách & giám sát (console preferences)
  Param(
    'pref:ui_label_mode',
    'Hiển thị trên mỗi máy',
    kind: ParamKind.choice,
    page: pageRoster,
    def: 'both',
    options: {
      'both': 'Tên và địa chỉ IP',
      'name': 'Chỉ tên máy',
      'ip': 'Chỉ địa chỉ IP',
    },
    help: 'Áp dụng cho dạng lưới, danh sách và danh sách gọn.',
  ),
  Param(
    'pref:ui_click',
    'Khi nhấp vào một máy',
    kind: ParamKind.choice,
    page: pageRoster,
    def: 'observe',
    options: {
      'observe': 'Mở cửa sổ quan sát',
      'select': 'Chỉ chọn máy (không mở gì)',
    },
    help:
        'Chọn "Chỉ chọn máy" nếu hay bấm nhầm. Chuột phải và nút ⋮ vẫn mở menu thao tác.',
  ),
  Param(
    'pref:view',
    'Kiểu xem khi mở chương trình',
    kind: ParamKind.choice,
    page: pageRoster,
    def: 'grid',
    options: {
      'grid': 'Dạng lưới',
      'list': 'Dạng danh sách',
      'compact': 'Danh sách gọn',
    },
  ),
  Param(
    'pref:sort',
    'Sắp xếp mặc định',
    kind: ParamKind.choice,
    page: pageRoster,
    def: 'none',
    options: {
      'none': 'Mặc định',
      'name_asc': 'Tên A → Z',
      'name_desc': 'Tên Z → A',
      'ip_asc': 'IP tăng dần',
      'ip_desc': 'IP giảm dần',
    },
  ),
  Param(
    'pref:grid_size',
    'Cỡ ô ở dạng lưới',
    kind: ParamKind.integer,
    page: pageRoster,
    def: 300,
    min: 180,
    max: 480,
    unit: 'px',
    help:
        'Chiều rộng mong muốn của mỗi ô. Cũng chỉnh được bằng thanh trượt ở thanh công cụ.',
  ),
  Param(
    'pref:show_offline',
    'Hiện cả máy đang offline',
    kind: ParamKind.toggle,
    page: pageRoster,
    def: true,
  ),
  Param(
    'pref:ui_stale_seconds',
    'Báo "ảnh màn hình đã cũ" sau',
    kind: ParamKind.integer,
    page: pageRoster,
    def: 10,
    min: 5,
    max: 120,
    unit: 'giây',
    help:
        'Nếu máy không gửi ảnh mới trong khoảng này, chấm trạng thái chuyển sang vàng.',
    advanced: true,
  ),
  Param(
    'activity.alert_cooldown_seconds',
    'Khoảng cách tối thiểu giữa hai cảnh báo mất kết nối',
    kind: ParamKind.integer,
    page: pageRoster,
    def: 30,
    min: 10,
    max: 3600,
    unit: 'giây',
    help: 'Một máy chập chờn liên tục chỉ tạo một cảnh báo trong khoảng này.',
    advanced: true,
  ),

  // ---- Theo dõi hoạt động
  Param(
    'activity.scan_seconds',
    'Làm mới hoạt động mỗi',
    kind: ParamKind.integer,
    page: pageActivity,
    def: 6,
    min: 3,
    max: 120,
    unit: 'giây',
    help: 'Ngắn hơn thì thấy nhanh hơn nhưng máy học viên làm việc nhiều hơn.',
  ),
  Param(
    'activity.live_apps',
    'Số ứng dụng hiện trên mỗi ô',
    kind: ParamKind.choice,
    page: pageActivity,
    def: 3,
    options: {1: '1', 2: '2', 3: '3', 4: '4', 5: '5'},
    help:
        'Các ứng dụng đang mở phía trước, kèm địa chỉ trang nếu đó là trình duyệt.',
  ),
  Param(
    'activity.collect_clipboard',
    'Ghi lại nội dung sao chép (clipboard)',
    kind: ParamKind.toggle,
    page: pageActivity,
    def: true,
    help:
        'Tắt nếu không cần theo dõi việc sao chép văn bản. Nội dung đã ghi sẽ bị xóa.',
  ),
  Param(
    'activity.collect_downloads',
    'Ghi lại các tệp tải xuống',
    kind: ParamKind.toggle,
    page: pageActivity,
    def: true,
  ),

  Param(
    'activity.evidence',
    'Tự chụp ảnh minh chứng khi học viên dùng website AI',
    kind: ParamKind.toggle,
    page: pageActivity,
    def: true,
    help:
        'Chỉ khi "Theo dõi hoạt động" đang bật và website AI chưa bị chặn. Ảnh được chụp vào lúc học viên đang thật sự dùng (không chụp lúc trang vừa tải xong), gửi về máy giảng viên và tự xóa khi qua ngày.',
  ),
  Param(
    'activity.evidence_per_site',
    'Số ảnh tối đa cho mỗi website AI mỗi học viên mỗi ngày',
    kind: ParamKind.integer,
    page: pageActivity,
    def: 3,
    min: 1,
    max: 10,
    unit: 'ảnh',
  ),
  Param(
    'activity.evidence_first_seconds',
    'Chụp ảnh đầu tiên sau khi dùng liên tục ít nhất',
    kind: ParamKind.integer,
    page: pageActivity,
    def: 60,
    min: 20,
    max: 600,
    unit: 'giây',
    advanced: true,
    help:
        'Áp dụng khi trang không đổi nội dung. Nếu trang đổi tiêu đề/đường dẫn (bắt đầu một cuộc trò chuyện) thì chụp sớm hơn, sau khoảng 15 giây.',
  ),
  Param(
    'activity.evidence_gap_seconds',
    'Khoảng cách tối thiểu giữa hai ảnh của cùng một website',
    kind: ParamKind.integer,
    page: pageActivity,
    def: 180,
    min: 60,
    max: 1800,
    unit: 'giây',
    advanced: true,
  ),

  // ---- Quy tắc lớp
  Param(
    'class.power_grace_seconds',
    'Báo trước khi tắt máy / khởi động lại',
    kind: ParamKind.integer,
    page: pageRules,
    def: 15,
    min: 0,
    max: 300,
    unit: 'giây',
    help:
        'Học viên thấy thông báo trong khoảng thời gian này trước khi máy tắt. 0 = tắt ngay.',
  ),

  // ---- Ghi màn hình
  Param(
    'tools.recording_max_minutes',
    'Thời gian ghi tối đa mỗi phiên',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 60,
    min: 5,
    max: 480,
    unit: 'phút',
    help: 'Phiên ghi tự dừng khi đủ thời gian này, phòng khi quên tắt.',
  ),
  Param(
    'recording.height',
    'Chất lượng hình',
    kind: ParamKind.choice,
    page: pageRecording,
    def: 720,
    options: {
      360: '360p — nhẹ nhất',
      480: '480p',
      720: '720p — cân bằng (mặc định)',
      1080: '1080p — nét nhất, nặng',
    },
    help: 'Chiều cao của video, giữ nguyên tỉ lệ màn hình học viên.',
  ),
  Param(
    'recording.fps',
    'Số khung hình mỗi giây',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 30,
    min: 5,
    max: 30,
    unit: 'khung/giây',
    help:
        'Giảm xuống 10–15 để video nhẹ hơn khi chỉ cần xem nội dung chậm như slide.',
  ),
  Param(
    'recording.bitrate_kbps',
    'Dung lượng video (bitrate)',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 3000,
    min: 500,
    max: 12000,
    unit: 'kbps',
    help: 'Càng cao càng nét và nặng. 3000 kbps ≈ 1,3 GB mỗi giờ mỗi máy.',
  ),
  Param(
    'recording.chunk_minutes',
    'Độ dài mỗi đoạn MP4',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 10,
    min: 1,
    max: 30,
    unit: 'phút',
    help:
        'Video tự cắt thành từng đoạn; đoạn nhỏ tải về sớm hơn và an toàn hơn. Đoạn × bitrate không được vượt 100000.',
  ),
  Param(
    'recording.folder',
    'Thư mục lưu video',
    kind: ParamKind.text,
    page: pageRecording,
    def: '',
    hint: 'Để trống = lưu ở Desktop',
    help:
        'Nhập đường dẫn đầy đủ, ví dụ D:\\GhiHinh. Nên là ổ trong máy: ổ mạng chậm hoặc mất kết nối sẽ làm ghi hình lỗi.',
  ),
  Param(
    'recording.teacher_stop_mb',
    'Dừng ghi khi ổ đĩa giảng viên còn dưới',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 1024,
    min: 256,
    max: 1048576,
    unit: 'MB',
    advanced: true,
  ),
  Param(
    'recording.teacher_warn_mb',
    'Cảnh báo khi ổ đĩa giảng viên còn dưới',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 5120,
    min: 256,
    max: 2097152,
    unit: 'MB',
    advanced: true,
    help: 'Phải lớn hơn hoặc bằng ngưỡng dừng.',
  ),
  Param(
    'recording.student_stop_mb',
    'Dừng ghi khi ổ đĩa máy học viên còn dưới',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 512,
    min: 256,
    max: 1048576,
    unit: 'MB',
    advanced: true,
  ),
  Param(
    'recording.student_warn_mb',
    'Cảnh báo khi ổ đĩa máy học viên còn dưới',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 2048,
    min: 256,
    max: 2097152,
    unit: 'MB',
    advanced: true,
  ),
  Param(
    'recording.gone_minutes',
    'Tự dừng khi máy học viên mất liên lạc với giảng viên quá',
    kind: ParamKind.integer,
    page: pageRecording,
    def: 30,
    min: 0,
    max: 240,
    unit: 'phút',
    advanced: true,
    help:
        '0 = không bao giờ tự dừng vì lý do này; nếu không, tối thiểu 5 phút.',
  ),

  // ---- Truyền tệp
  Param(
    'transfer.rate_kib_s',
    'Tốc độ gửi tệp tới mỗi máy',
    kind: ParamKind.choice,
    page: pageTransfer,
    def: 1000,
    options: {
      500: '0,5 MB/s — rất nhẹ cho mạng',
      1000: '1 MB/s (mặc định)',
      2000: '2 MB/s',
      5000: '5 MB/s',
      10000: '10 MB/s',
      20000: '20 MB/s — mạng dây tốt',
    },
    help: 'Tốc độ cho mỗi lượt gửi. Tăng lên khi gửi tệp lớn qua mạng dây.',
  ),

  // ---- Thông báo
  Param(
    'pref:ui_toast_ok_seconds',
    'Thông báo thành công hiện trong',
    kind: ParamKind.integer,
    page: pageNotices,
    def: 5,
    min: 2,
    max: 60,
    unit: 'giây',
  ),
  Param(
    'pref:ui_toast_error_seconds',
    'Thông báo lỗi hiện trong',
    kind: ParamKind.integer,
    page: pageNotices,
    def: 10,
    min: 3,
    max: 120,
    unit: 'giây',
  ),
  Param(
    'pref:ui_max_replies',
    'Số phản hồi của học viên được giữ lại',
    kind: ParamKind.integer,
    page: pageNotices,
    def: 300,
    min: 20,
    max: 2000,
    help: 'Phản hồi cũ hơn sẽ bị bỏ khi vượt số này.',
  ),

  // ---- Phát màn hình nâng cao (cần kết nối lại)
  Param(
    'video.jitter_buffer_ms',
    'Bộ đệm chống giật phía học viên',
    kind: ParamKind.integer,
    page: pageStream,
    def: 20,
    min: 0,
    max: 200,
    unit: 'ms',
    advanced: true,
    hot: false,
    help: 'Tăng khi Wi-Fi/mạng làm hình giật; đổi lại độ trễ cao hơn.',
  ),
  Param(
    'video.nack_repair_ms',
    'Thời gian chờ khôi phục gói tin mất',
    kind: ParamKind.integer,
    page: pageStream,
    def: 25,
    min: 0,
    max: 500,
    unit: 'ms',
    advanced: true,
    hot: false,
  ),
  Param(
    'video.keyframe_interval_ms',
    'Chu kỳ khung hình đầy đủ',
    kind: ParamKind.integer,
    page: pageStream,
    def: 4000,
    min: 1000,
    max: 10000,
    unit: 'ms',
    advanced: true,
    hot: false,
    help:
        'Ngắn hơn giúp máy vào lớp muộn thấy hình sớm hơn nhưng tốn băng thông.',
  ),
];

/// A problem found while checking the values.
class ParamProblem {
  const ParamProblem(this.page, this.message);
  final int page;
  final String message;
}

/// Current values of every [Param], with edit tracking, validation and the
/// pieces the Save button needs.
class ParamStore extends ChangeNotifier {
  ParamStore({
    required Map<String, dynamic> settings,
    required Map<String, dynamic> preferences,
    this.params = kParams,
  }) {
    for (final p in params) {
      final Object? raw = p.isPref
          ? preferences[p.key]
          : (settings[p.section] as Map?)?[p.key];
      final value = _coerce(p, raw) ?? p.def;
      _initial[p.id] = value;
      _values[p.id] = value;
      if (p.kind == ParamKind.integer || p.kind == ParamKind.text) {
        controllers[p.id] = TextEditingController(text: '$value')
          ..addListener(() => _onText(p));
      }
    }
  }

  final List<Param> params;
  final _values = <String, Object>{};
  final _initial = <String, Object>{};
  final controllers = <String, TextEditingController>{};

  Param byId(String id) => params.firstWhere((p) => p.id == id);

  static Object? _coerce(Param p, Object? raw) {
    if (raw == null) return null;
    switch (p.kind) {
      case ParamKind.integer:
        return raw is num ? raw.toInt() : int.tryParse('$raw');
      case ParamKind.toggle:
        return raw is bool ? raw : null;
      case ParamKind.text:
        return '$raw';
      case ParamKind.choice:
        for (final key in p.options!.keys) {
          if (key == raw || '$key' == '$raw') return key;
        }
        return null;
    }
  }

  void _onText(Param p) {
    final text = controllers[p.id]!.text.trim();
    if (p.kind == ParamKind.integer) {
      final n = int.tryParse(text);
      if (n != null) _values[p.id] = n;
    } else {
      _values[p.id] = text;
    }
    notifyListeners();
  }

  Object value(String id) => _values[id]!;
  int intValue(String id) => _values[id]! as int;

  /// Whether the text box currently holds something that is not a whole number.
  bool _unparsed(Param p) =>
      p.kind == ParamKind.integer &&
      int.tryParse(controllers[p.id]!.text.trim()) == null;

  void set(String id, Object value) {
    final p = byId(id);
    _values[id] = value;
    controllers[id]?.text = '$value';
    notifyListeners();
    assert(p.id == id);
  }

  bool isDefault(String id) =>
      !_unparsed(byId(id)) && _values[id] == byId(id).def;
  bool isChanged(String id) =>
      _unparsed(byId(id)) || _values[id] != _initial[id];

  void reset(String id) => set(id, byId(id).def);
  void resetAll(Iterable<String> ids) {
    for (final id in ids) {
      set(id, byId(id).def);
    }
  }

  bool get dirty => params.any((p) => isChanged(p.id));
  bool get needsRestart => params.any((p) => !p.hot && isChanged(p.id));

  /// Null when everything is acceptable.
  ParamProblem? validate() {
    for (final p in params) {
      if (p.kind != ParamKind.integer) continue;
      final n = int.tryParse(controllers[p.id]!.text.trim());
      if (n == null || n < p.min! || n > p.max!) {
        return ParamProblem(
          p.page,
          '${p.label}: nhập số nguyên từ ${p.min} đến ${p.max}${p.unit.isEmpty ? '' : ' ${p.unit}'}.',
        );
      }
    }
    int v(String id) => _values[id]! as int;
    if (v('recording.teacher_warn_mb') < v('recording.teacher_stop_mb') ||
        v('recording.student_warn_mb') < v('recording.student_stop_mb')) {
      return const ParamProblem(
        pageRecording,
        'Ngưỡng cảnh báo ổ đĩa phải lớn hơn hoặc bằng ngưỡng dừng ghi.',
      );
    }
    if (v('recording.gone_minutes') != 0 && v('recording.gone_minutes') < 5) {
      return const ParamProblem(
        pageRecording,
        'Thời gian chờ mất liên lạc phải là 0 (không dừng) hoặc từ 5 phút.',
      );
    }
    if (v('recording.chunk_minutes') * v('recording.bitrate_kbps') > 100000) {
      return const ParamProblem(
        pageRecording,
        'Đoạn video quá lớn: giảm độ dài mỗi đoạn hoặc bitrate (đoạn × bitrate tối đa 100000).',
      );
    }
    final folder = _values['recording.folder']! as String;
    if (folder.isNotEmpty &&
        !(RegExp(r'^[A-Za-z]:[\\/]').hasMatch(folder) ||
            folder.startsWith('/') ||
            folder.startsWith('\\\\'))) {
      return const ParamProblem(
        pageRecording,
        'Thư mục lưu video phải là đường dẫn đầy đủ, ví dụ D:\\GhiHinh (hoặc để trống để lưu ở Desktop).',
      );
    }
    return null;
  }

  /// Configuration sections to merge into the overlay that Save stores:
  /// `{section: {key: value}}` for every non-preference parameter.
  Map<String, Map<String, Object>> configSections() {
    final out = <String, Map<String, Object>>{};
    for (final p in params) {
      if (p.isPref) continue;
      (out[p.section] ??= {})[p.key] = _values[p.id]!;
    }
    return out;
  }

  /// Preferences to remember (only those that changed).
  Map<String, Object> changedPreferences() => {
    for (final p in params)
      if (p.isPref && isChanged(p.id)) p.key: _values[p.id]!,
  };

  @override
  void dispose() {
    for (final c in controllers.values) {
      c.dispose();
    }
    super.dispose();
  }
}
