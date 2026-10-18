import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_icons.dart' show SiteIcon;
import 'app_dialog.dart';
import 'evidence_view.dart';
import 'icon_tints.dart';

/// The AI / chatbot website list, as the Teacher's last snapshot reports it:
/// `{sites: [...], blocking: bool, default_count: n}`.
class AiSitesState {
  const AiSitesState(this.sites, this.blocking, this.defaultCount);
  final List<String> sites;
  final bool blocking;
  final int defaultCount;

  static AiSitesState of(Map tools) {
    final ai = tools['ai'];
    if (ai is! Map) return const AiSitesState([], false, 0);
    return AiSitesState(
      [for (final s in (ai['sites'] as List? ?? const [])) '$s'],
      ai['blocking'] == true,
      (ai['default_count'] as num?)?.toInt() ?? 0,
    );
  }
}

/// The host an entry stands for (`https://www.example.com/a` → `www.example.com`).
String aiHostOf(String entry) {
  var text = entry.trim();
  final scheme = text.indexOf('://');
  if (scheme >= 0) text = text.substring(scheme + 3);
  final end = text.indexOf(RegExp(r'[/?#:]'));
  return (end < 0 ? text : text.substring(0, end)).toLowerCase();
}

/// An entry written with a path (`github.com/features/copilot`): the whole
/// site is blocked, because the network cannot see paths.
bool aiEntryHasPath(String entry) {
  var text = entry.trim();
  final scheme = text.indexOf('://');
  if (scheme >= 0) text = text.substring(scheme + 3);
  final slash = text.indexOf('/');
  return slash >= 0 && slash < text.length - 1;
}

/// What the Teacher typed, if it is usable as a website entry; otherwise why not.
String? aiEntryProblem(
  String raw,
  Iterable<String> existing, {
  String? except,
}) {
  final text = raw.trim();
  if (text.isEmpty) return 'Nhập tên website, ví dụ chatgpt.com';
  final host = aiHostOf(text);
  if (!RegExp(
    r'^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$',
  ).hasMatch(host)) {
    return 'Tên website không hợp lệ: “$host”';
  }
  for (final other in existing) {
    if (other != except && aiHostOf(other) == host) {
      return '“$host” đã có trong danh sách';
    }
  }
  return null;
}

Future<void> showAiSites(BuildContext context, Agent agent) => showDialog<void>(
  context: context,
  builder: (_) => AiSitesDialog(agent: agent),
);

class AiSitesDialog extends StatefulWidget {
  const AiSitesDialog({super.key, required this.agent});
  final Agent agent;
  @override
  State<AiSitesDialog> createState() => _AiSitesDialogState();
}

class _AiSitesDialogState extends State<AiSitesDialog> {
  final add = TextEditingController();
  final addFocus = FocusNode();
  final search = TextEditingController();
  String? error;
  bool busy = false;

  Agent get agent => widget.agent;
  AiSitesState get ai =>
      AiSitesState.of((agent.state['tools'] as Map?) ?? const {});

  @override
  void dispose() {
    add.dispose();
    addFocus.dispose();
    search.dispose();
    super.dispose();
  }

  Future<void> run(Future<void> Function() action) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      await action();
    } catch (failure) {
      if (mounted) setState(() => error = '$failure');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  /// Sends a command and turns a refusal into a message the Teacher can read
  /// (the specific reason, when the Teacher agent gave one).
  Future<void> send(String action, Map<String, dynamic> input) async {
    final result = await agent.request(action, input: input);
    if (result['ok'] != true) throw Agent.failure(result);
  }

  Future<void> saveList(List<String> sites) =>
      run(() => send('ai_sites_set', {'sites': sites}));

  Future<void> addSite() async {
    final problem = aiEntryProblem(add.text, ai.sites);
    if (problem != null) {
      setState(() => error = problem);
      return;
    }
    await saveList([...ai.sites, add.text.trim()]);
    if (mounted && error == null) add.clear();
  }

  Future<void> edit(String entry) async {
    final result = await showDialog<String>(
      context: context,
      builder: (_) => _EditSiteDialog(entry: entry, existing: ai.sites),
    );
    if (result == null || result == entry) return;
    await saveList([for (final s in ai.sites) s == entry ? result : s]);
  }

  Future<void> remove(String entry) => saveList([
    for (final s in ai.sites)
      if (s != entry) s,
  ]);

  Future<void> reset() async {
    final ok = await confirmDialog(
      context,
      title: 'Khôi phục danh sách gốc?',
      icon: Icons.restart_alt,
      confirmLabel: 'Khôi phục',
      message:
          'Danh sách sẽ trở về ${ai.defaultCount} website ban đầu. Các website bạn đã thêm hoặc sửa sẽ mất.',
    );
    if (!ok || !mounted) return;
    await run(() => send('ai_sites_reset', {}));
  }

  Future<void> setBlocking(bool on) =>
      run(() => send('ai_block_set', {'enabled': on}));

  /// The "add" box: as the Teacher types, sites already in the list that match
  /// are offered (and a repeat is flagged at once), so nothing is typed twice.
  Widget _addRow(AiSitesState state) {
    return ListenableBuilder(
      listenable: add,
      builder: (context, _) {
        final typed = add.text.trim();
        final problem = typed.isEmpty
            ? null
            : aiEntryProblem(typed, state.sites);
        // Only a repeat is worth flagging while typing; "not a site yet" is
        // normal until the name is complete.
        final repeat = problem != null && problem.contains('đã có');
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(
              child: LayoutBuilder(
                builder: (context, box) => RawAutocomplete<String>(
                  textEditingController: add,
                  focusNode: addFocus,
                  displayStringForOption: aiHostOf,
                  optionsBuilder: (value) {
                    final q = value.text.trim().toLowerCase();
                    if (q.length < 2) return const Iterable<String>.empty();
                    return state.sites
                        .where((s) => s.toLowerCase().contains(q))
                        .take(6);
                  },
                  onSelected: (entry) {
                    // Already there: show it in the list instead of adding it again.
                    add.clear();
                    search.text = aiHostOf(entry);
                    setState(
                      () => error =
                          '“${aiHostOf(entry)}” đã có sẵn trong danh sách.',
                    );
                  },
                  fieldViewBuilder: (context, controller, focus, submit) =>
                      TextField(
                        key: const ValueKey('ai-add'),
                        controller: controller,
                        focusNode: focus,
                        onSubmitted: (_) => addSite(),
                        decoration: InputDecoration(
                          isDense: true,
                          labelText: 'Thêm website',
                          hintText: 'ví dụ: chatgpt.com',
                          errorText: repeat ? problem : null,
                        ),
                      ),
                  optionsViewBuilder: (context, onSelected, options) => Align(
                    alignment: Alignment.topLeft,
                    child: Material(
                      elevation: 4,
                      borderRadius: BorderRadius.circular(8),
                      child: ConstrainedBox(
                        constraints: BoxConstraints(
                          maxWidth: box.maxWidth,
                          maxHeight: 240,
                        ),
                        child: ListView(
                          key: const ValueKey('ai-suggestions'),
                          padding: EdgeInsets.zero,
                          shrinkWrap: true,
                          children: [
                            for (final entry in options)
                              ListTile(
                                key: ValueKey('ai-suggest-$entry'),
                                dense: true,
                                leading: SiteIcon(
                                  url: 'https://${aiHostOf(entry)}',
                                  size: 18,
                                ),
                                title: Text(aiHostOf(entry)),
                                trailing: const Text(
                                  'Đã có trong danh sách',
                                  style: TextStyle(
                                    fontSize: 11,
                                    color: AppDialog.muted,
                                  ),
                                ),
                                onTap: () => onSelected(entry),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
            const SizedBox(width: 8),
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: FilledButton.icon(
                key: const ValueKey('ai-add-button'),
                onPressed: busy || typed.isEmpty || repeat ? null : addSite,
                icon: const Icon(Icons.add, size: 18),
                label: const Text('Thêm'),
              ),
            ),
          ],
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: Listenable.merge([agent, search]),
      builder: (context, _) {
        final state = ai;
        final query = search.text.trim().toLowerCase();
        final shown = [
          for (final s in state.sites)
            if (query.isEmpty || s.toLowerCase().contains(query)) s,
        ];
        return AppDialog(
          icon: Icons.smart_toy_outlined,
          title: 'Website AI & chatbot',
          subtitle:
              'Danh sách các dịch vụ AI. Bật chặn để học viên không vào được; theo dõi hoạt động sẽ báo khi có máy đang dùng.',
          width: 760,
          height: 640,
          scrollable: false,
          bodyPadding: const EdgeInsets.fromLTRB(24, 8, 24, 0),
          footerLeading: Wrap(
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              TextButton.icon(
                key: const ValueKey('ai-reset'),
                onPressed: busy ? null : reset,
                icon: const TIcon(Icons.restart_alt, size: 18),
                label: const Text('Khôi phục danh sách gốc'),
              ),
              TextButton.icon(
                key: const ValueKey('ai-evidence'),
                onPressed: () => showEvidence(context, agent),
                icon: const TIcon(Icons.photo_camera_outlined, size: 18),
                label: Text(
                  'Minh chứng (${evidenceItems((agent.state['tools'] as Map?) ?? const {}).length})',
                ),
              ),
            ],
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Container(
                decoration: BoxDecoration(
                  color: state.blocking
                      ? const Color(0x14b83b40)
                      : const Color(0xfff6f8fa),
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(
                    color: state.blocking
                        ? const Color(0x55b83b40)
                        : AppDialog.border,
                  ),
                ),
                child: SwitchListTile(
                  key: const ValueKey('ai-block-switch'),
                  value: state.blocking,
                  onChanged: busy || (state.sites.isEmpty && !state.blocking)
                      ? null
                      : setBlocking,
                  title: const Text(
                    'Chặn tất cả website AI trong danh sách',
                    style: TextStyle(fontWeight: FontWeight.w700),
                  ),
                  subtitle: Text(
                    state.blocking
                        ? 'Đang chặn ${state.sites.length} website trên máy học viên (đến hết ngày hôm nay). Tắt công tắc để cho phép lại.'
                        : 'Đang tắt: học viên vào được các website này.',
                  ),
                ),
              ),
              const SizedBox(height: 8),
              const Text(
                'Một website bị chặn nếu nó nằm trong danh sách này (khi công tắc bật) hoặc trong mục Internet — nếu hai nơi mâu thuẫn thì ưu tiên chặn. '
                'Website ghi kèm đường dẫn (có nhãn “cả trang”) sẽ chặn toàn bộ trang đó, vì mạng không phân biệt được đường dẫn.',
                style: TextStyle(
                  fontSize: 12,
                  height: 1.4,
                  color: AppDialog.muted,
                ),
              ),
              if (error != null) ...[
                const SizedBox(height: 8),
                InlineNotice(error!),
              ],
              const SizedBox(height: 10),
              _addRow(state),
              const SizedBox(height: 8),
              TextField(
                key: const ValueKey('ai-search'),
                controller: search,
                decoration: InputDecoration(
                  isDense: true,
                  prefixIcon: const Icon(Icons.search, size: 18),
                  hintText: 'Tìm trong ${state.sites.length} website',
                ),
              ),
              const SizedBox(height: 6),
              Expanded(
                child: state.sites.isEmpty
                    ? const DialogEmpty(
                        icon: Icons.smart_toy_outlined,
                        title: 'Danh sách đang trống',
                        message:
                            'Thêm website ở trên, hoặc bấm “Khôi phục danh sách gốc”.',
                      )
                    : shown.isEmpty
                    ? const Center(child: Text('Không có website nào khớp'))
                    : ListView.separated(
                        itemCount: shown.length,
                        separatorBuilder: (_, _) => const Divider(height: 1),
                        itemBuilder: (_, i) {
                          final entry = shown[i];
                          return ListTile(
                            key: ValueKey('ai-row-$entry'),
                            dense: true,
                            contentPadding: EdgeInsets.zero,
                            leading: SiteIcon(
                              url: 'https://${aiHostOf(entry)}',
                              size: 22,
                            ),
                            title: Row(
                              children: [
                                Flexible(
                                  child: Text(
                                    aiHostOf(entry),
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: const TextStyle(
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                ),
                                if (aiEntryHasPath(entry)) ...[
                                  const SizedBox(width: 8),
                                  Tooltip(
                                    message:
                                        'Ghi là $entry nhưng sẽ chặn cả trang ${aiHostOf(entry)}',
                                    child: Container(
                                      padding: const EdgeInsets.symmetric(
                                        horizontal: 6,
                                        vertical: 1,
                                      ),
                                      decoration: BoxDecoration(
                                        color: const Color(0x1ac2410c),
                                        borderRadius: BorderRadius.circular(8),
                                      ),
                                      child: const Text(
                                        'cả trang',
                                        style: TextStyle(
                                          fontSize: 10.5,
                                          color: Color(0xffc2410c),
                                        ),
                                      ),
                                    ),
                                  ),
                                ],
                              ],
                            ),
                            subtitle: aiEntryHasPath(entry)
                                ? Text(
                                    entry,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: const TextStyle(fontSize: 11.5),
                                  )
                                : null,
                            trailing: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                IconButton(
                                  key: ValueKey('ai-edit-$entry'),
                                  tooltip: 'Sửa',
                                  visualDensity: VisualDensity.compact,
                                  onPressed: busy ? null : () => edit(entry),
                                  icon: const TIcon(
                                    Icons.edit_outlined,
                                    size: 18,
                                  ),
                                ),
                                IconButton(
                                  key: ValueKey('ai-delete-$entry'),
                                  tooltip: 'Xóa khỏi danh sách',
                                  visualDensity: VisualDensity.compact,
                                  onPressed: busy ? null : () => remove(entry),
                                  icon: const TIcon(
                                    Icons.delete_outline,
                                    size: 18,
                                    color: Color(0xffb83b40),
                                  ),
                                ),
                              ],
                            ),
                          );
                        },
                      ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// Edits one entry; owns its text field so the field outlives the closing animation.
class _EditSiteDialog extends StatefulWidget {
  const _EditSiteDialog({required this.entry, required this.existing});
  final String entry;
  final List<String> existing;
  @override
  State<_EditSiteDialog> createState() => _EditSiteDialogState();
}

class _EditSiteDialogState extends State<_EditSiteDialog> {
  late final controller = TextEditingController(text: widget.entry);
  String? problem;

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  void submit() {
    final found = aiEntryProblem(
      controller.text,
      widget.existing,
      except: widget.entry,
    );
    if (found != null) {
      setState(() => problem = found);
    } else {
      Navigator.pop(context, controller.text.trim());
    }
  }

  @override
  Widget build(BuildContext context) => AppDialog(
    icon: Icons.edit_outlined,
    title: 'Sửa website',
    width: 460,
    actions: [
      AppDialog.primary(label: 'Lưu', icon: Icons.check, onPressed: submit),
    ],
    child: TextField(
      controller: controller,
      autofocus: true,
      onSubmitted: (_) => submit(),
      decoration: InputDecoration(labelText: 'Website', errorText: problem),
    ),
  );
}
