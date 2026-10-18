import 'icon_tints.dart';
import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'app_icons.dart';
import 'window_shell.dart';
import 'website_dependencies.dart';

const _teal = Color(0xff156b79);
const _danger = Color(0xffb83b40);

/// A saved, named list of sites (one rule per line; `*` wildcards allowed).
class SiteList {
  SiteList({
    required this.id,
    required this.name,
    required this.rules,
    List<WebsiteGroup>? groups,
  }) : groups = groups ?? [];
  final String id;
  String name, rules;
  final List<WebsiteGroup> groups;
  factory SiteList.fromJson(Map<dynamic, dynamic> j) => SiteList(
    id: '${j['id']}',
    name: '${j['name'] ?? 'Danh sách'}',
    rules: '${j['rules'] ?? ''}',
    groups: [
      for (final g in j['groups'] as List? ?? [])
        if (g is Map) WebsiteGroup.fromJson(g),
    ],
  );
  Map<String, dynamic> toJson() => {
    'id': id,
    'name': name,
    'rules': rules,
    'groups': groups.map((g) => g.toJson()).toList(),
  };
}

class _SavedWebsite {
  const _SavedWebsite(this.list, this.group);
  final SiteList list;
  final WebsiteGroup group;
}

const _presets = <String, String>{
  'Mạng xã hội':
      'facebook.com\n*.fbcdn.net\nmessenger.com\ninstagram.com\ntiktok.com\n*.tiktokcdn.com\ntwitter.com\nx.com\nsnapchat.com\nreddit.com\npinterest.com\ntelegram.org\nzalo.me\n*.zalo.me\ndiscord.com',
  'Video & game':
      'youtube.com\n*.googlevideo.com\nnetflix.com\ntwitch.tv\nsteampowered.com\n*.steamcontent.com\nroblox.com\nepicgames.com',
  'Trợ lý AI':
      'chatgpt.com\nopenai.com\nclaude.ai\ngemini.google.com\ncopilot.microsoft.com\nperplexity.ai\ncharacter.ai',
};

class _Mode {
  const _Mode(this.id, this.label, this.detail, this.icon, this.color);
  final String id, label, detail;
  final IconData icon;
  final Color color;
}

const _modes = <_Mode>[
  _Mode(
    'open',
    'Cho phép tất cả',
    'Gỡ mọi hạn chế cho các máy đã chọn.',
    Icons.public,
    Color(0xff1f7a4d),
  ),
  _Mode(
    'block_all',
    'Chặn tất cả',
    'Không có Internet. Máy này (localhost) và mạng lớp học vẫn hoạt động '
        'nên bài tập web cục bộ và kết nối với giảng viên không bị ảnh hưởng.',
    Icons.public_off,
    _danger,
  ),
  _Mode(
    'allow_only',
    'Chỉ cho phép các trang trong danh sách',
    'Mọi trang khác bị chặn.',
    Icons.playlist_add_check,
    _teal,
  ),
  _Mode(
    'block_only',
    'Chặn các trang trong danh sách',
    'Mọi trang khác vẫn mở.',
    Icons.block,
    Color(0xffb86b22),
  ),
];

/// Readable result of a website dependency scan.
String scanStatusLabel(String status) => switch (status) {
  'scanned' => 'Đã quét',
  'partial' => 'Quét một phần',
  'timeout' => 'Hết thời gian quét',
  'cancelled' => 'Đã dừng quét',
  'error' => 'Lỗi quét',
  _ => 'Chưa quét',
};

String modeLabel(String mode) => switch (mode) {
  'block_all' => 'Chặn tất cả',
  'allow_only' => 'Chỉ cho phép danh sách',
  'block_only' => 'Chặn danh sách',
  _ => 'Mở',
};

Map<String, String> restrictedStudents(Agent agent) {
  final map = (agent.state['tools'] as Map? ?? const {})['internet'];
  return {
    if (map is Map)
      for (final e in map.entries) '${e.key}': '${e.value}',
  };
}

class InternetWindow extends StatefulWidget {
  const InternetWindow({
    super.key,
    required this.agent,
    required this.studentIds,
    required this.audience,
    this.classWide = true,
  });

  /// Whether this covers the whole class (so computers that join later get
  /// the rule too) rather than a hand-picked group.
  final bool classWide;
  final Agent agent;
  final List<String> studentIds;
  final String audience;
  @override
  State<InternetWindow> createState() => _InternetWindowState();
}

class _InternetWindowState extends State<InternetWindow> {
  List<SiteList> lists = [];
  late final text = TextEditingController();
  final website = TextEditingController();
  final websiteFocus = FocusNode();
  WebsiteScanner? scanner;
  WebsiteGroup? scanning;
  String? scanNotice;
  String browserPath = '';
  Future<void> _saveChain = Future.value();
  bool get scanningNow => scanner != null;
  String mode = 'block_all';
  String? listId;
  int lease = 240;
  bool busy = false;
  bool applied = false;

  /// Signature of the policy last confirmed for these Students in this window.
  String? lastApplied;
  String? error;
  List<Map<String, String>> skipped = [];

  Agent get agent => widget.agent;

  /// Same rules `apply` would send for the current editor contents.
  List<String> currentRules() =>
      (mode == 'allow_only'
              ? effectiveWebsiteRules(text.text, current?.groups ?? [])
              : text.text.split('\n'))
          .map((l) => l.trim())
          .where((l) => l.isNotEmpty)
          .toList();

  static String policySignature(String mode, List<String> rules) =>
      jsonEncode([mode, rules]);
  String get signature => policySignature(
    mode,
    mode == 'allow_only' || mode == 'block_only' ? currentRules() : const [],
  );

  String get scopeKey => widget.classWide
      ? 'class'
      : (List.of(widget.studentIds)..sort()).join(',');
  bool get targetsRestricted {
    final restricted = restrictedStudents(agent);
    return widget.studentIds.any(restricted.containsKey);
  }

  Map get storedPolicies =>
      agent.preferences['internet_applied'] as Map? ?? const {};

  /// What is in force now for these Students, or null when it is not known.
  String? get baselineSignature {
    if (lastApplied != null) return lastApplied;
    if (!targetsRestricted) return policySignature('open', const []);
    final stored = storedPolicies[scopeKey];
    return stored is String ? stored : null;
  }

  /// Apply is only useful when the editor differs from what is already in force.
  bool get changed {
    final baseline = baselineSignature;
    return baseline == null || signature != baseline;
  }

  bool get listMode => mode == 'allow_only' || mode == 'block_only';
  SiteList? get current {
    for (final l in lists) {
      if (l.id == listId) return l;
    }
    return null;
  }

  List<_SavedWebsite> get savedWebsites => [
    for (final list in lists)
      for (final group in list.groups) _SavedWebsite(list, group),
  ];

  _SavedWebsite? findSavedWebsite(String value) {
    try {
      final host = websiteUri(value).host.toLowerCase();
      for (final saved in savedWebsites) {
        if (saved.group.host == host) return saved;
      }
    } on FormatException {
      // Partial or invalid input has no exact match yet.
    }
    return null;
  }

  void selectSavedWebsite(_SavedWebsite saved) {
    if (listId != saved.list.id) select(saved.list.id);
    website.text = saved.group.url;
    setState(() {
      scanNotice =
          'Already saved in “${saved.list.name}”. Use the saved group or rescan it.';
    });
  }

  @override
  void initState() {
    super.initState();
    // Same race as the VS Code window: `agent.preferences` is not loaded
    // yet at this widget's first build (Agent.start() finishes later), so
    // reading it unconditionally here always saw the empty default —
    // every reopen defaulted to Block all with no saved lists. Defer to
    // once the agent is actually ready.
    if (agent.connected) {
      applyPreferences();
    } else {
      agent.addListener(waitForAgent);
    }
  }

  void waitForAgent() {
    if (!agent.connected) return;
    agent.removeListener(waitForAgent);
    if (!mounted) return;
    setState(applyPreferences);
  }

  void applyPreferences() {
    browserPath = agent.preferences['website_scan_browser'] as String? ?? '';
    lists = [
      for (final l
          in (agent.preferences['internet_lists'] as List? ?? const []))
        if (l is Map) SiteList.fromJson(l),
    ];
    final last = agent.preferences['internet_last'];
    if (last is Map) {
      mode = '${last['mode'] ?? mode}';
      lease = (last['lease'] as num?)?.toInt() ?? lease;
      listId = last['list'] as String?;
    }
    // Open on what is actually in force, so nothing looks pending until the
    // Teacher changes something.
    if (!targetsRestricted) {
      mode = 'open';
    } else if (storedPolicies[scopeKey] case final String stored) {
      try {
        mode = '${(jsonDecode(stored) as List).first}';
      } catch (_) {}
    }
    if (current == null) listId = lists.isEmpty ? null : lists.first.id;
    text.text = current?.rules ?? '';
  }

  @override
  void dispose() {
    agent.removeListener(waitForAgent);
    scanner?.cancel();
    website.dispose();
    websiteFocus.dispose();
    text.dispose();
    super.dispose();
  }

  Future<void> saveLists() async {
    current?.rules = text.text;
    final changes = <String, dynamic>{
      'internet_lists': lists.map((l) => l.toJson()).toList(),
      'internet_last': {'mode': mode, 'lease': lease, 'list': listId},
      'website_scan_browser': browserPath,
    };
    final previous = _saveChain;
    final next = () async {
      try {
        await previous;
      } catch (_) {}
      await agent.savePreferences(changes);
    }();
    _saveChain = next;
    await next;
  }

  void select(String? id) {
    current?.rules = text.text;
    setState(() {
      listId = id;
      text.text = current?.rules ?? '';
    });
    saveLists();
  }

  Future<String?> askName(String title, String initial) async {
    final c = TextEditingController(text: initial);
    final name = await showDialog<String>(
      context: context,
      builder: (context) => AppDialog(
        icon: Icons.list_alt_outlined,
        title: title,
        width: 440,
        actions: [
          AppDialog.cancel(context),
          ListenableBuilder(
            listenable: c,
            builder: (context, _) => AppDialog.primary(
              // A new or renamed list needs a name that is not just the old one.
              onPressed:
                  c.text.trim().isEmpty || c.text.trim() == initial.trim()
                  ? null
                  : () => Navigator.pop(context, c.text),
              label: 'OK',
            ),
          ),
        ],
        child: TextField(
          controller: c,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Tên danh sách'),
          onSubmitted: (v) => v.trim().isEmpty || v.trim() == initial.trim()
              ? null
              : Navigator.pop(context, v),
        ),
      ),
    );
    disposeAfterClose(c);
    final trimmed = name?.trim() ?? '';
    return trimmed.isEmpty ? null : trimmed;
  }

  Future<void> newList([String starter = '']) async {
    final name = await askName('Danh sách mới', '');
    if (name == null) return;
    current?.rules = text.text;
    final list = SiteList(
      id: DateTime.now().microsecondsSinceEpoch.toRadixString(36),
      name: name,
      rules: starter,
    );
    setState(() {
      lists.add(list);
      listId = list.id;
      text.text = starter;
    });
    saveLists();
  }

  Future<void> rename() async {
    final list = current;
    if (list == null) return;
    final name = await askName('Đổi tên danh sách', list.name);
    if (name == null) return;
    setState(() => list.name = name);
    saveLists();
  }

  Future<void> delete() async {
    final list = current;
    if (list == null) return;
    setState(() {
      lists.remove(list);
      listId = lists.isEmpty ? null : lists.first.id;
      text.text = current?.rules ?? '';
    });
    saveLists();
  }

  void addPreset(String name) {
    final rules = _presets[name]!;
    if (current == null) {
      newList(rules);
      return;
    }
    final existing = text.text.trim();
    setState(() => text.text = existing.isEmpty ? rules : '$existing\n$rules');
  }

  void toggleDependency(WebsiteDependency dependency, bool allowed) {
    setState(() {
      dependency.allowed = allowed;
      scanNotice = 'Đã lưu nhóm. Bấm “Áp dụng” để cập nhật học viên.';
    });
    saveLists();
  }

  void removeGroup(WebsiteGroup group) {
    setState(() {
      current!.groups.remove(group);
      scanNotice = 'Đã xóa nhóm. Bấm “Áp dụng” để cập nhật học viên.';
    });
    saveLists();
  }

  Future<void> configureBrowser() async {
    final c = TextEditingController(text: browserPath);
    final path = await showDialog<String>(
      context: context,
      builder: (context) => AppDialog(
        icon: Icons.web_outlined,
        title: 'Trình duyệt quét website',
        subtitle: 'Dùng để quét các tên miền mà một trang web cần.',
        width: 560,
        actions: [
          AppDialog.cancel(context),
          ListenableBuilder(
            listenable: c,
            builder: (context, _) => AppDialog.primary(
              onPressed: c.text.trim() == browserPath.trim()
                  ? null
                  : () => Navigator.pop(context, c.text),
              icon: Icons.check,
              label: 'Lưu',
            ),
          ),
        ],
        child: TextField(
          controller: c,
          decoration: const InputDecoration(
            labelText: 'Đường dẫn Chrome / Edge',
            helperText:
                'Để trống để tự phát hiện. Mỗi lần quét dùng một hồ sơ tạm riêng.',
          ),
          onSubmitted: (v) => Navigator.pop(context, v),
        ),
      ),
    );
    disposeAfterClose(c);
    if (path == null || !mounted) return;
    setState(() => browserPath = path.trim());
    await saveLists();
  }

  Future<void> allowWebsite() async {
    if (scanningNow || busy) return;
    try {
      final uri = websiteUri(website.text);
      final saved = findSavedWebsite(uri.toString());
      if (saved != null) {
        if (listId != saved.list.id) select(saved.list.id);
        setState(() {
          scanNotice =
              'Using the saved group in “${saved.list.name}”. Use Rescan to refresh it.';
        });
        await apply(closeAfter: false);
        return;
      }
      var list = current;
      if (list == null) {
        list = SiteList(
          id: DateTime.now().microsecondsSinceEpoch.toRadixString(36),
          name: 'Website được phép',
          rules: text.text,
        );
        setState(() {
          lists.add(list!);
          listId = list.id;
        });
      }
      final group = WebsiteGroup(uri.toString());
      setState(() {
        list!.groups.add(group);
        website.clear();
      });
      await saveLists();
      await scanGroup(group);
      if (mounted) await apply(closeAfter: false);
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  Future<void> scanGroup(WebsiteGroup group) async {
    if (scanningNow || busy) return;
    final worker = WebsiteScanner();
    setState(() {
      scanner = worker;
      scanning = group;
      error = null;
      scanNotice = null;
    });
    try {
      final result = await worker.scan(group.url, browserPath: browserPath);
      if (!mounted) return;
      setState(() {
        group.dependencies = result.dependencies;
        group.scannedAt = DateTime.now().toUtc();
        group.status = result.status;
        group.note = result.note;
        scanNotice = result.note;
      });
      await saveLists();
    } catch (e) {
      if (mounted) {
        setState(() {
          group.status = 'error';
          group.note = '$e';
          scanNotice =
              'Trang chính vẫn được phép; quét các tên miền phụ thuộc thất bại.';
          error = '$e';
        });
        await saveLists();
      }
    } finally {
      if (mounted) {
        setState(() {
          scanner = null;
          scanning = null;
        });
      }
    }
  }

  Future<void> apply({bool closeAfter = true}) async {
    if (busy || scanningNow) return;
    final rules = currentRules();
    final sent = signature;
    if (listMode && rules.every((l) => l.startsWith('#'))) {
      setState(() => error = 'Hãy thêm ít nhất một trang vào danh sách.');
      return;
    }
    await saveLists();
    if (!mounted) return;
    setState(() {
      busy = true;
      error = null;
    });
    final result = await agent.request(
      'tool_policy',
      selected: widget.studentIds,
      input: {
        'class_wide': widget.classWide,
        'policy': {
          'internet': {'mode': mode, 'rules': listMode ? rules : <String>[]},
          'lease_minutes': 1440,
        },
      },
      timeout: const Duration(seconds: 15),
    );
    if (!mounted) return;
    if (result['ok'] != true) {
      setState(() {
        busy = false;
        error = agent.error ?? Agent.failure(result);
      });
      return;
    }
    final skippedList = (result['result'] as Map?)?['skipped'];
    unawaited(
      agent.savePreferences({
        'internet_applied': {...storedPolicies, scopeKey: sent},
      }),
    );
    setState(() {
      lastApplied = sent;
      busy = false;
      applied = closeAfter;
      if (!closeAfter) scanNotice = 'Đã lưu nhóm website và gửi chính sách.';
      skipped = [
        if (skippedList is List)
          for (final s in skippedList)
            if (s is Map)
              {'name': '${s['name'] ?? ''}', 'ip': '${s['ip'] ?? ''}'},
      ];
    });
    // The roster already shows an ongoing restricted-state icon per
    // computer; this window's job is done once the command is accepted —
    // keeping it open afterward is a dead end for the Teacher, so it closes
    // itself, with a brief confirmation first. If some computers were
    // skipped, stay open so the Teacher can actually read the list instead
    // of it flashing by.
    if (skipped.isEmpty && closeAfter) {
      await Future<void>.delayed(const Duration(milliseconds: 900));
      if (mounted) await agent.finishApplication();
    }
  }

  @override
  Widget build(BuildContext context) {
    final restricted = restrictedStudents(agent).length;
    final selectedMode = _modes.firstWhere((m) => m.id == mode);
    return WindowShell(
      icon: Icons.public_off_outlined,
      tone: AppTone.danger,
      title: 'Truy cập Internet · ${widget.audience}',
      subtitle: restricted == 0
          ? 'Hiện chưa có máy nào bị hạn chế'
          : '$restricted máy đang bị hạn chế',
      footer: _footer(),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _ModeSidebar(
            mode: mode,
            busy: busy || scanningNow,
            onSelect: (m) => setState(() => mode = m),
          ),
          const VerticalDivider(width: 1, color: AppDialog.border),
          Expanded(
            child: AbsorbPointer(
              absorbing: busy || applied,
              child: Opacity(
                opacity: busy || applied ? 0.6 : 1,
                child: Padding(
                  padding: const EdgeInsets.all(22),
                  child: listMode
                      ? _ListEditor(state: this, mode: selectedMode)
                      : _SimpleModePanel(mode: selectedMode),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _footer() => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    mainAxisSize: MainAxisSize.min,
    children: [
      if (scanNotice != null)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: InlineNotice(scanNotice!, tone: AppTone.info),
        ),
      if (error != null)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: InlineNotice(error!),
        ),
      if (skipped.isNotEmpty)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: InlineNotice(
            'Đã áp dụng cho phần còn lại của lớp, nhưng bỏ qua ${skipped.length} máy không thể thực thi quy tắc Internet (cần bản mới chạy bằng quyền quản trị): '
            '${skipped.map((s) => '${s['name']} (${s['ip']})').join(', ')}',
            tone: AppTone.warning,
          ),
        ),
      Row(
        children: [
          const Expanded(
            child: Text(
              'Quy tắc vẫn có hiệu lực khi giảng viên mất kết nối hoặc khởi động lại, và với máy vào lớp hoặc khởi động lại sau — cho tới khi bạn đổi hoặc hết ngày (nửa đêm).',
              style: TextStyle(
                fontSize: 12,
                height: 1.4,
                color: AppDialog.muted,
              ),
            ),
          ),
          if (applied)
            const Padding(
              padding: EdgeInsets.only(right: 12),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TIcon(Icons.check_circle, size: 17, color: Color(0xff1f7a4d)),
                  SizedBox(width: 6),
                  Text(
                    'Đã áp dụng',
                    style: TextStyle(
                      color: Color(0xff1f7a4d),
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ],
              ),
            ),
          AppDialog.cancel(
            context,
            label: applied ? 'Đóng' : 'Hủy',
            enabled: !busy,
            onPressed: () {
              saveLists();
              agent.finishApplication();
            },
          ),
          const SizedBox(width: 8),
          ListenableBuilder(
            // The rules editor is a controller, not setState-driven.
            listenable: text,
            builder: (context, _) => AppDialog.primary(
              onPressed: busy || applied || scanningNow || !changed
                  ? null
                  : () => apply(),
              icon: Icons.check,
              busy: busy,
              label: busy ? 'Đang áp dụng…' : 'Áp dụng',
            ),
          ),
        ],
      ),
    ],
  );
}

class _ModeSidebar extends StatelessWidget {
  const _ModeSidebar({
    required this.mode,
    required this.busy,
    required this.onSelect,
  });
  final String mode;
  final bool busy;
  final ValueChanged<String> onSelect;
  @override
  Widget build(BuildContext context) => Container(
    width: 280,
    color: Colors.white,
    padding: const EdgeInsets.symmetric(vertical: 12),
    child: ListView(
      children: [
        for (final m in _modes)
          _ModeTile(
            mode: m,
            selected: mode == m.id,
            onTap: busy ? null : () => onSelect(m.id),
          ),
      ],
    ),
  );
}

class _ModeTile extends StatelessWidget {
  const _ModeTile({
    required this.mode,
    required this.selected,
    required this.onTap,
  });
  final _Mode mode;
  final bool selected;
  final VoidCallback? onTap;
  @override
  Widget build(BuildContext context) => InkWell(
    onTap: onTap,
    child: Container(
      margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: selected ? mode.color.withValues(alpha: 0.1) : null,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(
          color: selected
              ? mode.color.withValues(alpha: 0.4)
              : Colors.transparent,
        ),
      ),
      child: Row(
        children: [
          TIcon(
            mode.icon,
            size: 20,
            color: selected ? mode.color : Colors.blueGrey,
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              mode.label,
              style: TextStyle(
                fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
                fontSize: 13,
                color: selected ? mode.color : Colors.black87,
              ),
            ),
          ),
        ],
      ),
    ),
  );
}

/// Allow all / Block all: nothing to configure, so the panel explains the
/// effect clearly instead of leaving an empty list editor on screen.
class _SimpleModePanel extends StatelessWidget {
  const _SimpleModePanel({required this.mode});
  final _Mode mode;
  @override
  Widget build(BuildContext context) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 72,
          height: 72,
          decoration: BoxDecoration(
            color: mode.color.withValues(alpha: 0.12),
            shape: BoxShape.circle,
          ),
          child: TIcon(mode.icon, size: 34, color: mode.color),
        ),
        const SizedBox(height: 18),
        Text(
          mode.label,
          style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
        ),
        const SizedBox(height: 8),
        SizedBox(
          width: 360,
          child: Text(
            mode.detail,
            textAlign: TextAlign.center,
            style: const TextStyle(
              fontSize: 13,
              color: Colors.blueGrey,
              height: 1.4,
            ),
          ),
        ),
      ],
    ),
  );
}

class _ListEditor extends StatelessWidget {
  const _ListEditor({required this.state, required this.mode});
  final _InternetWindowState state;
  final _Mode mode;
  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Row(
        children: [
          TIcon(mode.icon, size: 17, color: mode.color),
          const SizedBox(width: 8),
          Text(
            mode.label,
            style: TextStyle(
              fontWeight: FontWeight.w700,
              fontSize: 14,
              color: mode.color,
            ),
          ),
        ],
      ),
      const SizedBox(height: 4),
      Text(
        mode.detail,
        style: const TextStyle(fontSize: 12, color: Colors.blueGrey),
      ),
      const SizedBox(height: 14),
      Row(
        children: [
          Expanded(
            child: DropdownButtonFormField<String>(
              initialValue: state.current?.id,
              isDense: true,
              decoration: const InputDecoration(
                labelText: 'Danh sách đã lưu',
                isDense: true,
              ),
              items: [
                for (final l in state.lists)
                  DropdownMenuItem(value: l.id, child: Text(l.name)),
              ],
              onChanged: state.scanningNow ? null : state.select,
            ),
          ),
          IconButton(
            tooltip: 'Danh sách mới',
            icon: const TIcon(Icons.add),
            onPressed: state.scanningNow ? null : state.newList,
          ),
          IconButton(
            tooltip: 'Đổi tên',
            icon: const TIcon(Icons.edit_outlined),
            onPressed: state.current == null || state.scanningNow
                ? null
                : state.rename,
          ),
          IconButton(
            tooltip: 'Xóa danh sách',
            icon: const TIcon(Icons.delete_outline),
            onPressed: state.current == null || state.scanningNow
                ? null
                : state.delete,
          ),
          PopupMenuButton<String>(
            tooltip: 'Thêm bộ trang có sẵn',
            icon: const TIcon(Icons.playlist_add),
            onSelected: state.scanningNow ? null : state.addPreset,
            itemBuilder: (_) => [
              for (final p in _presets.keys)
                PopupMenuItem(value: p, child: Text('Thêm: $p')),
            ],
          ),
        ],
      ),
      const SizedBox(height: 10),
      Expanded(
        child: state.mode == 'allow_only'
            ? _WebsiteGroups(state: state)
            : TextField(
                controller: state.text,
                expands: true,
                maxLines: null,
                minLines: null,
                textAlignVertical: TextAlignVertical.top,
                style: const TextStyle(fontFamily: 'Menlo', fontSize: 12.5),
                decoration: const InputDecoration(
                  hintText:
                      'Mỗi dòng một trang, ví dụ:\nwikipedia.org\n*.edu.vn\n203.0.113.0/24\n# dòng bắt đầu bằng # là ghi chú',
                ),
              ),
      ),
      const SizedBox(height: 8),
      const Text(
        'example.com gồm cả trang con · *.example.com chỉ trang con · '
        '* khớp mọi chuỗi. Áp dụng cho mọi chương trình trên máy, nhưng '
        'HTTPS chỉ thấy tên trang, không thấy phần sau dấu /.',
        style: TextStyle(fontSize: 11.5, color: Colors.blueGrey),
      ),
    ],
  );
}

class _WebsiteGroups extends StatelessWidget {
  const _WebsiteGroups({required this.state});
  final _InternetWindowState state;
  @override
  Widget build(BuildContext context) => Column(
    children: [
      Row(
        children: [
          Expanded(
            child: Autocomplete<_SavedWebsite>(
              textEditingController: state.website,
              focusNode: state.websiteFocus,
              displayStringForOption: (saved) => saved.group.url,
              optionsBuilder: (value) {
                final query = value.text.trim().toLowerCase();
                if (query.isEmpty) return const [];
                final normalized = query.replaceFirst(
                  RegExp(r'^https?://'),
                  '',
                );
                return state.savedWebsites
                    .where(
                      (saved) =>
                          saved.group.url.toLowerCase().contains(query) ||
                          saved.group.host.contains(normalized),
                    )
                    .take(8);
              },
              onSelected: state.selectSavedWebsite,
              fieldViewBuilder: (context, controller, focusNode, onSubmitted) =>
                  TextField(
                    controller: controller,
                    focusNode: focusNode,
                    enabled: !state.scanningNow,
                    decoration: const InputDecoration(
                      labelText: 'Website cần cho phép',
                      hintText: 'https://flutter.dev',
                      isDense: true,
                      suffixIcon: TIcon(Icons.arrow_drop_down),
                    ),
                    onSubmitted: (_) => state.allowWebsite(),
                  ),
              optionsViewBuilder: (context, onSelected, options) {
                final saved = options.toList();
                return Align(
                  alignment: Alignment.topLeft,
                  child: Material(
                    elevation: 5,
                    borderRadius: BorderRadius.circular(8),
                    clipBehavior: Clip.antiAlias,
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(
                        maxWidth: 560,
                        maxHeight: 240,
                      ),
                      child: ListView.separated(
                        padding: EdgeInsets.zero,
                        shrinkWrap: true,
                        itemCount: saved.length,
                        separatorBuilder: (_, _) => const Divider(height: 1),
                        itemBuilder: (context, index) {
                          final item = saved[index];
                          final highlighted =
                              AutocompleteHighlightedOption.of(context) ==
                              index;
                          return ListTile(
                            dense: true,
                            selected: highlighted,
                            leading: SiteIcon(url: item.group.url, size: 22),
                            title: Text(
                              item.group.url,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            subtitle: Text(
                              'Đã lưu trong ${item.list.name} · '
                              '${item.group.dependencies.length} tên miền phụ thuộc',
                            ),
                            trailing: const TIcon(Icons.check_circle_outline),
                            onTap: () => onSelected(item),
                          );
                        },
                      ),
                    ),
                  ),
                );
              },
            ),
          ),
          const SizedBox(width: 8),
          FilledButton.icon(
            onPressed: state.scanningNow ? null : state.allowWebsite,
            icon: const TIcon(Icons.add_link, size: 18),
            label: Text(
              state.findSavedWebsite(state.website.text) == null
                  ? 'Cho phép website'
                  : 'Dùng nhóm đã lưu',
            ),
          ),
          IconButton(
            tooltip: 'Cấu hình Chrome / Edge',
            onPressed: state.scanningNow ? null : state.configureBrowser,
            icon: const TIcon(Icons.settings_outlined, size: 19),
          ),
        ],
      ),
      const SizedBox(height: 6),
      if (state.scanningNow)
        Row(
          children: [
            const SizedBox(
              width: 14,
              height: 14,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                'Đang quét ${state.scanning!.host} · tối đa 15 giây',
                style: const TextStyle(fontSize: 12),
              ),
            ),
            TextButton(
              onPressed: () => state.scanner?.cancel(),
              child: const Text('Dừng'),
            ),
          ],
        )
      else
        const Align(
          alignment: Alignment.centerLeft,
          child: Text(
            'Các tên miền tài nguyên tĩnh được thêm tự động. Nhóm đã lưu được dùng lại, không cần quét.',
            style: TextStyle(fontSize: 11.5, color: Colors.blueGrey),
          ),
        ),
      const SizedBox(height: 6),
      Expanded(
        child: ListView(
          children: [
            if (state.current?.groups.isEmpty ?? true)
              const Padding(
                padding: EdgeInsets.all(20),
                child: Text(
                  'Hãy thêm một website ở trên, hoặc nhập quy tắc thủ công bên dưới.',
                  textAlign: TextAlign.center,
                  style: TextStyle(color: Colors.blueGrey),
                ),
              ),
            for (final g in state.current?.groups ?? <WebsiteGroup>[])
              Card(
                elevation: 0,
                color: Colors.white,
                clipBehavior: Clip.antiAlias,
                child: ExpansionTile(
                  key: ValueKey(g.url),
                  tilePadding: const EdgeInsets.symmetric(horizontal: 10),
                  leading: SiteIcon(url: g.url, size: 24),
                  title: Text(
                    g.host,
                    style: const TextStyle(
                      fontWeight: FontWeight.w600,
                      fontSize: 13,
                    ),
                  ),
                  subtitle: Text(
                    'Được phép trong danh sách · ${g.dependencies.where((d) => d.allowed).length} tên miền hỗ trợ · '
                    '${scanStatusLabel(g.status)}',
                    style: const TextStyle(fontSize: 11),
                  ),
                  childrenPadding: const EdgeInsets.fromLTRB(12, 0, 12, 8),
                  children: [
                    Align(
                      alignment: Alignment.centerLeft,
                      child: Text(
                        g.url,
                        style: const TextStyle(
                          fontSize: 11,
                          color: Colors.blueGrey,
                        ),
                      ),
                    ),
                    if (g.scannedAt != null)
                      Align(
                        alignment: Alignment.centerLeft,
                        child: Text(
                          'Lần quét gần nhất: ${g.scannedAt!.toLocal().toString().split('.').first}',
                          style: const TextStyle(fontSize: 11),
                        ),
                      ),
                    if (g.note.isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.only(top: 6),
                        child: Text(
                          g.note,
                          style: const TextStyle(fontSize: 11),
                        ),
                      ),
                    for (final d in g.dependencies)
                      CheckboxListTile(
                        dense: true,
                        visualDensity: VisualDensity.compact,
                        contentPadding: EdgeInsets.zero,
                        controlAffinity: ListTileControlAffinity.leading,
                        value: d.allowed,
                        title: Text(
                          d.host,
                          style: const TextStyle(fontSize: 12),
                        ),
                        subtitle: Text(
                          (d.types.toList()..sort()).join(', '),
                          style: const TextStyle(fontSize: 10.5),
                        ),
                        onChanged: state.scanningNow
                            ? null
                            : (v) => state.toggleDependency(d, v!),
                      ),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.end,
                      children: [
                        TextButton.icon(
                          onPressed: state.scanningNow
                              ? null
                              : () async {
                                  await state.scanGroup(g);
                                  if (state.mounted && state.error == null) {
                                    await state.apply(closeAfter: false);
                                  }
                                },
                          icon: const TIcon(Icons.refresh, size: 16),
                          label: const Text('Quét lại'),
                        ),
                        TextButton.icon(
                          onPressed: state.scanningNow
                              ? null
                              : () => state.removeGroup(g),
                          icon: const TIcon(Icons.delete_outline, size: 16),
                          label: const Text('Xóa'),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ExpansionTile(
              title: const Text(
                'Quy tắc thủ công',
                style: TextStyle(fontSize: 12),
              ),
              subtitle: const Text(
                'Quy tắc có sẵn, ký tự đại diện và dải IP',
                style: TextStyle(fontSize: 11),
              ),
              children: [
                TextField(
                  controller: state.text,
                  enabled: !state.scanningNow,
                  minLines: 4,
                  maxLines: 6,
                  style: const TextStyle(fontFamily: 'Menlo', fontSize: 12),
                  decoration: const InputDecoration(
                    hintText: 'Mỗi dòng một trang',
                  ),
                  onChanged: (_) => state.saveLists(),
                ),
              ],
            ),
          ],
        ),
      ),
    ],
  );
}
