import 'dart:async';
import 'package:flutter/material.dart';
import 'agent.dart';

const _teal = Color(0xff156b79);
const _bg = Color(0xfff3f5f9);
const _danger = Color(0xffb83b40);

/// A saved, named list of sites (one rule per line; `*` wildcards allowed).
class SiteList {
  SiteList({required this.id, required this.name, required this.rules});
  final String id;
  String name, rules;
  factory SiteList.fromJson(Map<dynamic, dynamic> j) => SiteList(
    id: '${j['id']}',
    name: '${j['name'] ?? 'List'}',
    rules: '${j['rules'] ?? ''}',
  );
  Map<String, dynamic> toJson() => {'id': id, 'name': name, 'rules': rules};
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
  });
  final Agent agent;
  final List<String> studentIds;
  final String audience;
  @override
  State<InternetWindow> createState() => _InternetWindowState();
}

class _InternetWindowState extends State<InternetWindow> {
  late List<SiteList> lists = [
    for (final l in (agent.preferences['internet_lists'] as List? ?? const []))
      if (l is Map) SiteList.fromJson(l),
  ];
  late final text = TextEditingController();
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

  @override
  void initState() {
    super.initState();
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
    text.dispose();
    super.dispose();
  }

  void saveLists() {
    current?.rules = text.text;
    agent.savePreferences({
      'internet_lists': lists.map((l) => l.toJson()).toList(),
      'internet_last': {'mode': mode, 'lease': lease, 'list': listId},
    });
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

  Future<void> apply() async {
    if (busy) return;
    final rules = text.text
        .split('\n')
        .map((l) => l.trim())
        .where((l) => l.isNotEmpty)
        .toList();
    if (listMode && rules.every((l) => l.startsWith('#'))) {
      setState(() => error = 'Add at least one site to the list.');
      return;
    }
    saveLists();
    setState(() {
      busy = true;
      error = null;
    });
    final result = await agent.request(
      'tool_policy',
      selected: widget.studentIds,
      input: {
        'policy': {
          'internet': {'mode': mode, 'rules': listMode ? rules : <String>[]},
          'lease_minutes': lease,
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
      applied = true;
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
    if (skipped.isEmpty) {
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
                    busy: busy,
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
                      const Text(
                        'Keep rules if the Teacher disconnects for',
                        style: TextStyle(fontSize: 12.5),
                      ),
                      const SizedBox(width: 8),
                      DropdownButton<int>(
                        value: lease,
                        isDense: true,
                        underline: const SizedBox.shrink(),
                        items: const [
                          DropdownMenuItem(value: 30, child: Text('30 min')),
                          DropdownMenuItem(value: 120, child: Text('2 hours')),
                          DropdownMenuItem(value: 240, child: Text('4 hours')),
                          DropdownMenuItem(value: 480, child: Text('8 hours')),
                          DropdownMenuItem(
                            value: 1440,
                            child: Text('24 hours'),
                          ),
                        ],
                        onChanged: busy
                            ? null
                            : (v) => setState(() => lease = v!),
                      ),
                      const Spacer(),
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
                        onPressed: busy || applied ? null : apply,
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
              onChanged: state.select,
            ),
          ),
          IconButton(
            tooltip: 'New list',
            icon: const Icon(Icons.add),
            onPressed: state.newList,
          ),
          IconButton(
            tooltip: 'Rename',
            icon: const Icon(Icons.edit_outlined),
            onPressed: state.current == null ? null : state.rename,
          ),
          IconButton(
            tooltip: 'Delete list',
            icon: const Icon(Icons.delete_outline),
            onPressed: state.current == null ? null : state.delete,
          ),
          PopupMenuButton<String>(
            tooltip: 'Add a ready-made set of sites',
            icon: const Icon(Icons.playlist_add),
            onSelected: state.addPreset,
            itemBuilder: (_) => [
              for (final p in _presets.keys)
                PopupMenuItem(value: p, child: Text('Add: $p')),
            ],
          ),
        ],
      ),
      const SizedBox(height: 10),
      Expanded(
        child: TextField(
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
