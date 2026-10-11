import 'dart:async';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'website_dependencies.dart';

const _teal = Color(0xff156b79);
const _bg = Color(0xfff3f5f9);
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
    name: '${j['name'] ?? 'List'}',
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
  'Social media':
      'facebook.com\n*.fbcdn.net\nmessenger.com\ninstagram.com\ntiktok.com\n*.tiktokcdn.com\ntwitter.com\nx.com\nsnapchat.com\nreddit.com\npinterest.com\ntelegram.org\nzalo.me\n*.zalo.me\ndiscord.com',
  'Video & games':
      'youtube.com\n*.googlevideo.com\nnetflix.com\ntwitch.tv\nsteampowered.com\n*.steamcontent.com\nroblox.com\nepicgames.com',
  'AI assistants':
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
    'Allow all',
    'Remove every restriction for the selected computers.',
    Icons.public,
    Color(0xff1f7a4d),
  ),
  _Mode(
    'block_all',
    'Block all',
    'No Internet at all. This computer (localhost) and the classroom '
        'network keep working, so local web exercises and the Teacher '
        'connection are unaffected.',
    Icons.public_off,
    _danger,
  ),
  _Mode(
    'allow_only',
    'Allow only listed sites',
    'Everything else is blocked.',
    Icons.playlist_add_check,
    _teal,
  ),
  _Mode(
    'block_only',
    'Block listed sites',
    'Everything else stays open.',
    Icons.block,
    Color(0xffb86b22),
  ),
];

String modeLabel(String mode) => switch (mode) {
  'block_all' => 'Blocked',
  'allow_only' => 'Allow list',
  'block_only' => 'Block list',
  _ => 'Open',
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
  String? error;
  List<Map<String, String>> skipped = [];

  Agent get agent => widget.agent;
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
    if (current == null) listId = lists.isEmpty ? null : lists.first.id;
    text.text = current?.rules ?? '';
  }

  @override
  void dispose() {
    agent.removeListener(waitForAgent);
    scanner?.cancel();
    website.dispose();
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
      builder: (context) => AlertDialog(
        title: Text(title),
        content: TextField(
          controller: c,
          autofocus: true,
          decoration: const InputDecoration(
            labelText: 'List name',
            border: OutlineInputBorder(),
          ),
          onSubmitted: (v) => Navigator.pop(context, v),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, c.text),
            child: const Text('OK'),
          ),
        ],
      ),
    );
    c.dispose();
    final trimmed = name?.trim() ?? '';
    return trimmed.isEmpty ? null : trimmed;
  }

  Future<void> newList([String starter = '']) async {
    final name = await askName('New list', '');
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
    final name = await askName('Rename list', list.name);
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
      scanNotice = 'Group saved. Press Apply to update Students.';
    });
    saveLists();
  }

  void removeGroup(WebsiteGroup group) {
    setState(() {
      current!.groups.remove(group);
      scanNotice = 'Group removed. Press Apply to update Students.';
    });
    saveLists();
  }

  Future<void> configureBrowser() async {
    final c = TextEditingController(text: browserPath);
    final path = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Headless browser'),
        content: SizedBox(
          width: 520,
          child: TextField(
            controller: c,
            decoration: const InputDecoration(
              labelText: 'Chrome / Edge executable path',
              helperText:
                  'Leave empty to detect automatically. Each scan uses a separate temporary profile.',
            ),
            onSubmitted: (v) => Navigator.pop(context, v),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, c.text),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    c.dispose();
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
          name: 'Allowed websites',
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
              'The main website will still be allowed; dependency scan failed.';
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
    final rules =
        (mode == 'allow_only'
                ? effectiveWebsiteRules(text.text, current?.groups ?? [])
                : text.text.split('\n'))
            .map((l) => l.trim())
            .where((l) => l.isNotEmpty)
            .toList();
    if (listMode && rules.every((l) => l.startsWith('#'))) {
      setState(() => error = 'Add at least one site to the list.');
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
    setState(() {
      busy = false;
      applied = closeAfter;
      if (!closeAfter) scanNotice = 'Website group saved and policy sent.';
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
    return Scaffold(
      backgroundColor: _bg,
      body: SafeArea(
        child: Column(
          children: [
            Material(
              color: Colors.white,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 10, 16, 10),
                child: Row(
                  children: [
                    const Icon(
                      Icons.public_off_outlined,
                      color: _teal,
                      size: 20,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        'Internet access · ${widget.audience}',
                        style: const TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.w600,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    Text(
                      restricted == 0
                          ? 'No computer is restricted right now'
                          : '$restricted computer${restricted == 1 ? '' : 's'} currently restricted',
                      style: const TextStyle(
                        fontSize: 12,
                        color: Colors.blueGrey,
                      ),
                    ),
                  ],
                ),
              ),
            ),
            const Divider(height: 1),
            Expanded(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _ModeSidebar(
                    mode: mode,
                    busy: busy || scanningNow,
                    onSelect: (m) => setState(() => mode = m),
                  ),
                  const VerticalDivider(width: 1),
                  Expanded(
                    child: AbsorbPointer(
                      absorbing: busy || applied,
                      child: Opacity(
                        opacity: busy || applied ? 0.6 : 1,
                        child: Padding(
                          padding: const EdgeInsets.all(20),
                          child: listMode
                              ? _ListEditor(state: this, mode: selectedMode)
                              : _SimpleModePanel(mode: selectedMode),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
            const Divider(height: 1),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 12, 20, 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      const Expanded(
                        child: Text(
                          'Rules stay in force if the Teacher disconnects or restarts, and for computers that join or restart later — until you change them or the day ends (midnight).',
                          style: TextStyle(
                            fontSize: 12,
                            color: Colors.blueGrey,
                          ),
                        ),
                      ),
                      if (applied)
                        const Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Icon(
                              Icons.check_circle,
                              size: 17,
                              color: Color(0xff1f7a4d),
                            ),
                            SizedBox(width: 6),
                            Text(
                              'Applied',
                              style: TextStyle(
                                color: Color(0xff1f7a4d),
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ],
                        ),
                    ],
                  ),
                  if (scanNotice != null) ...[
                    const SizedBox(height: 6),
                    Text(
                      scanNotice!,
                      style: const TextStyle(fontSize: 12, color: _teal),
                    ),
                  ],
                  if (error != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      error!,
                      style: const TextStyle(fontSize: 12.5, color: _danger),
                    ),
                  ],
                  if (skipped.isNotEmpty) ...[
                    const SizedBox(height: 10),
                    Container(
                      padding: const EdgeInsets.all(10),
                      decoration: BoxDecoration(
                        color: const Color(0xfffdf1e0),
                        borderRadius: BorderRadius.circular(8),
                        border: Border.all(color: const Color(0xffe8c57a)),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              const Icon(
                                Icons.warning_amber_rounded,
                                size: 16,
                                color: Color(0xffb86b22),
                              ),
                              const SizedBox(width: 6),
                              Text(
                                'Applied to the rest of the class, but skipped '
                                '${skipped.length} computer${skipped.length == 1 ? '' : 's'} '
                                'that cannot enforce Internet rules (needs the updated build running as administrator):',
                                style: const TextStyle(
                                  fontSize: 12,
                                  fontWeight: FontWeight.w600,
                                  color: Color(0xff8a4e14),
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(height: 6),
                          Wrap(
                            spacing: 10,
                            runSpacing: 4,
                            children: [
                              for (final s in skipped)
                                Text(
                                  '${s['name']} (${s['ip']})',
                                  style: const TextStyle(
                                    fontSize: 12,
                                    fontFamily: 'Menlo',
                                    color: Color(0xff8a4e14),
                                  ),
                                ),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ],
                  const SizedBox(height: 12),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      TextButton(
                        onPressed: busy
                            ? null
                            : () {
                                saveLists();
                                agent.finishApplication();
                              },
                        child: Text(applied ? 'Close' : 'Cancel'),
                      ),
                      const SizedBox(width: 8),
                      FilledButton.icon(
                        onPressed: busy || applied || scanningNow
                            ? null
                            : () => apply(),
                        icon: busy
                            ? const SizedBox(
                                width: 16,
                                height: 16,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                  color: Colors.white,
                                ),
                              )
                            : const Icon(Icons.check, size: 18),
                        label: Text(busy ? 'Applying…' : 'Apply'),
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
    width: 260,
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
          Icon(
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
          child: Icon(mode.icon, size: 34, color: mode.color),
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
          Icon(mode.icon, size: 17, color: mode.color),
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
                labelText: 'Saved list',
                border: OutlineInputBorder(),
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
            tooltip: 'New list',
            icon: const Icon(Icons.add),
            onPressed: state.scanningNow ? null : state.newList,
          ),
          IconButton(
            tooltip: 'Rename',
            icon: const Icon(Icons.edit_outlined),
            onPressed: state.current == null || state.scanningNow
                ? null
                : state.rename,
          ),
          IconButton(
            tooltip: 'Delete list',
            icon: const Icon(Icons.delete_outline),
            onPressed: state.current == null || state.scanningNow
                ? null
                : state.delete,
          ),
          PopupMenuButton<String>(
            tooltip: 'Add a ready-made set of sites',
            icon: const Icon(Icons.playlist_add),
            onSelected: state.scanningNow ? null : state.addPreset,
            itemBuilder: (_) => [
              for (final p in _presets.keys)
                PopupMenuItem(value: p, child: Text('Add: $p')),
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
                      'One site per line, e.g.\nwikipedia.org\n*.edu.vn\n203.0.113.0/24\n# lines starting with # are notes',
                  border: OutlineInputBorder(),
                ),
              ),
      ),
      const SizedBox(height: 8),
      const Text(
        'example.com covers its sub-sites too · *.example.com sub-sites only · '
        '* matches any text. Works for every program on the computer, but '
        'HTTPS shows only the site name, never the page after the /.',
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
                      labelText: 'Website to allow',
                      hintText: 'https://flutter.dev',
                      border: OutlineInputBorder(),
                      isDense: true,
                      suffixIcon: Icon(Icons.arrow_drop_down),
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
                            leading: const Icon(Icons.language, size: 19),
                            title: Text(
                              item.group.url,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            subtitle: Text(
                              'Already saved in ${item.list.name} · '
                              '${item.group.dependencies.length} dependency domains',
                            ),
                            trailing: const Icon(Icons.check_circle_outline),
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
            icon: const Icon(Icons.add_link, size: 18),
            label: Text(
              state.findSavedWebsite(state.website.text) == null
                  ? 'Allow website'
                  : 'Use saved group',
            ),
          ),
          IconButton(
            tooltip: 'Configure Chrome / Edge',
            onPressed: state.scanningNow ? null : state.configureBrowser,
            icon: const Icon(Icons.settings_outlined, size: 19),
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
                'Scanning ${state.scanning!.host} · up to 15s',
                style: const TextStyle(fontSize: 12),
              ),
            ),
            TextButton(
              onPressed: () => state.scanner?.cancel(),
              child: const Text('Stop'),
            ),
          ],
        )
      else
        const Align(
          alignment: Alignment.centerLeft,
          child: Text(
            'Static resource domains are added automatically. Saved groups are reused without scanning.',
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
                  'Add a website above, or enter manual rules below.',
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
                  leading: const Icon(Icons.language, color: _teal, size: 21),
                  title: Text(
                    g.host,
                    style: const TextStyle(
                      fontWeight: FontWeight.w600,
                      fontSize: 13,
                    ),
                  ),
                  subtitle: Text(
                    'Allowed in list · ${g.dependencies.where((d) => d.allowed).length} supporting domains · '
                    '${g.status == 'not_scanned' ? 'Not scanned' : g.status}',
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
                          'Last scan: ${g.scannedAt!.toLocal().toString().split('.').first}',
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
                          icon: const Icon(Icons.refresh, size: 16),
                          label: const Text('Rescan'),
                        ),
                        TextButton.icon(
                          onPressed: state.scanningNow
                              ? null
                              : () => state.removeGroup(g),
                          icon: const Icon(Icons.delete_outline, size: 16),
                          label: const Text('Remove'),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ExpansionTile(
              title: const Text('Manual rules', style: TextStyle(fontSize: 12)),
              subtitle: const Text(
                'Existing rules, wildcards and IP ranges',
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
                    hintText: 'One site per line',
                    border: OutlineInputBorder(),
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
