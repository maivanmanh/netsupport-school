import 'icon_tints.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'app_icons.dart';

Future<void> showWebsiteDialog(
  BuildContext context,
  Agent agent, {
  required List<String> studentIds,
  required String audience,
}) => showDialog<void>(
  context: context,
  builder: (_) =>
      _WebsiteDialog(agent: agent, ids: studentIds, audience: audience),
);

class _WebsiteDialog extends StatefulWidget {
  const _WebsiteDialog({
    required this.agent,
    required this.ids,
    required this.audience,
  });
  final Agent agent;
  final List<String> ids;
  final String audience;
  @override
  State<_WebsiteDialog> createState() => _WebsiteDialogState();
}

class _WebsiteDialogState extends State<_WebsiteDialog> {
  static const historyKey = 'recent_websites';
  final input = TextEditingController();
  final urlFocus = FocusNode();
  late List<String> recent =
      ((widget.agent.preferences[historyKey] as List?) ?? const [])
          .whereType<String>()
          .take(20)
          .toList();
  bool busy = false;
  String? error, jobId;

  List<String> get onlineTargets => widget.agent.students
      .where((s) => s['online'] == true && widget.ids.contains(s['id']))
      .map((s) => '${s['id']}')
      .toList();

  String normalize(String text) {
    var value = text.trim();
    if (value.isEmpty) throw const FormatException('Hãy nhập địa chỉ website.');
    if (!value.contains('://')) value = 'https://$value';
    final uri = Uri.tryParse(value);
    if (value.length > 4096 ||
        value.contains(RegExp(r'[\s\x00-\x1f\x7f]')) ||
        uri == null ||
        !['http', 'https'].contains(uri.scheme) ||
        uri.host.isEmpty ||
        uri.userInfo.isNotEmpty) {
      throw const FormatException(
        'Hãy dùng địa chỉ website hợp lệ bắt đầu bằng http:// hoặc https://.',
      );
    }
    return uri.toString();
  }

  Future<void> persist(List<String> values) async {
    if (!widget.agent.connected) {
      throw StateError(
        'Không lưu được: kho cài đặt của giảng viên chưa sẵn sàng.',
      );
    }
    // Await the committed settings update before dispatching the request.
    await widget.agent.savePreferences({historyKey: values});
  }

  Future<void> open([String? saved]) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final url = normalize(saved ?? input.text);
      final ids = onlineTargets;
      if (ids.isEmpty) {
        throw StateError('Không có máy nào đang online để nhận.');
      }
      final next = [url, ...recent.where((u) => u != url)].take(20).toList();
      await persist(next);
      if (mounted) {
        setState(() {
          recent = next;
          input.text = url;
        });
      }
      final response = await widget.agent.request(
        'tool_website',
        selected: ids,
        input: {'url': url},
      );
      if (response['ok'] != true) throw StateError(Agent.failure(response));
      if (mounted) setState(() => jobId = response['job_id'] as String?);
    } catch (e) {
      if (mounted) {
        setState(() => error = e is FormatException ? e.message : '$e');
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> remove(String url) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final next = recent.where((u) => u != url).toList();
      await persist(next);
      if (mounted) setState(() => recent = next);
    } catch (e) {
      if (mounted) {
        setState(() => error = 'Không lưu được danh sách website gần đây: $e');
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Widget urlInput() => LayoutBuilder(
    builder: (context, constraints) => RawAutocomplete<String>(
      textEditingController: input,
      focusNode: urlFocus,
      optionsBuilder: (value) {
        final query = value.text.trim().toLowerCase().replaceFirst(
          RegExp(r'^https?://'),
          '',
        );
        if (busy || query.isEmpty) return const Iterable<String>.empty();
        return recent
            .where(
              (url) => url
                  .toLowerCase()
                  .replaceFirst(RegExp(r'^https?://'), '')
                  .contains(query),
            )
            .take(6);
      },
      // Selecting a suggestion only fills the field. Open remains explicit.
      onSelected: (_) => setState(() => error = null),
      fieldViewBuilder: (context, controller, focus, submit) => TextField(
        controller: controller,
        focusNode: focus,
        autofocus: true,
        enabled: !busy,
        keyboardType: TextInputType.url,
        onSubmitted: (_) {
          submit();
          open();
        },
        decoration: InputDecoration(
          labelText: 'Địa chỉ website',
          hintText: 'https://example.com',
          prefixIcon: const TIcon(Icons.link),
          helperText: 'Nhập địa chỉ hoặc chọn một gợi ý đã lưu.',
        ),
      ),
      optionsViewBuilder: (context, select, options) => Align(
        alignment: Alignment.topLeft,
        child: Material(
          elevation: 6,
          clipBehavior: Clip.antiAlias,
          borderRadius: BorderRadius.circular(10),
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxWidth: constraints.maxWidth,
              maxHeight: 240,
            ),
            child: ListView.builder(
              padding: const EdgeInsets.symmetric(vertical: 4),
              shrinkWrap: true,
              itemCount: options.length,
              itemBuilder: (context, index) {
                final url = options.elementAt(index);
                return ListTile(
                  dense: true,
                  selected: AutocompleteHighlightedOption.of(context) == index,
                  selectedTileColor: const Color(0xffe4f1f2),
                  leading: const TIcon(Icons.history, size: 18),
                  title: Text(
                    url,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  onTap: () {
                    select(url);
                    urlFocus.requestFocus();
                  },
                );
              },
            ),
          ),
        ),
      ),
    ),
  );

  Widget deliveryStatus() {
    final jobs =
        ((widget.agent.state['tools'] as Map? ?? const {})['jobs'] as List?)
            ?.whereType<Map>();
    final job = jobs?.where((j) => j['id'] == jobId).firstOrNull;
    if (job == null) return const SizedBox.shrink();
    final targets = (job['targets'] as List).whereType<Map>().toList();
    final complete = targets.where((t) => t['status'] == 'complete').length;
    final failed = targets
        .where((t) => ['error', 'cancelled'].contains(t['status']))
        .toList();
    final waiting = targets.length - complete - failed.length;
    return Container(
      margin: const EdgeInsets.only(top: 16),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: failed.isEmpty
            ? const Color(0xffedf7f2)
            : const Color(0xfffff1f0),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          TIcon(
            failed.isEmpty ? Icons.check_circle_outline : Icons.error_outline,
            size: 20,
            color: failed.isEmpty
                ? const Color(0xff23734b)
                : const Color(0xffb83b40),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  waiting > 0
                      ? 'Đang mở… $complete/${targets.length}'
                      : 'Đã mở trên $complete máy${failed.isEmpty ? '' : ' · lỗi ${failed.length} máy'}',
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                if (failed.isNotEmpty) ...[
                  const SizedBox(height: 4),
                  Text('${failed.first['error']}'),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }

  @override
  void dispose() {
    input.dispose();
    urlFocus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AppDialog(
    icon: Icons.language,
    title: 'Mở website',
    subtitle: 'Mở trong trình duyệt mặc định của từng máy học viên.',
    width: 620,
    actions: [
      AppDialog.primary(
        onPressed: () => open(),
        icon: Icons.open_in_browser,
        busy: busy,
        label: busy ? 'Đang mở…' : 'Mở website',
      ),
    ],
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
          decoration: BoxDecoration(
            color: const Color(0xfff2f5f8),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Row(
            children: [
              const TIcon(Icons.computer_outlined, size: 18),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  'Gửi tới ${widget.audience}',
                  style: const TextStyle(fontSize: 13),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 18),
        urlInput(),
        if (error != null) ...[
          const SizedBox(height: 10),
          InlineNotice(error!),
        ],
        const SizedBox(height: 22),
        DialogSection(
          title: 'Website gần đây',
          hint: 'Bấm một địa chỉ để mở ngay trên các máy này.',
          padBottom: 0,
          child: recent.isEmpty
              ? const DialogPanel(
                  child: DialogEmpty(
                    icon: Icons.history,
                    title: 'Chưa có website gần đây',
                    message: 'Các địa chỉ bạn đã mở sẽ hiện ở đây.',
                  ),
                )
              : DialogPanel(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxHeight: 240),
                    child: ListView.separated(
                      shrinkWrap: true,
                      itemCount: recent.length,
                      separatorBuilder: (_, _) => const Divider(height: 1),
                      itemBuilder: (_, index) => ListTile(
                        contentPadding: const EdgeInsets.symmetric(
                          horizontal: 12,
                        ),
                        dense: true,
                        leading: SiteIcon(url: recent[index], size: 22),
                        title: Text(
                          recent[index],
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        subtitle: Text(
                          Uri.tryParse(recent[index])?.host ?? '',
                          maxLines: 1,
                        ),
                        onTap: busy ? null : () => open(recent[index]),
                        trailing: IconButton(
                          tooltip: 'Xóa khỏi danh sách gần đây',
                          icon: const TIcon(Icons.close, size: 17),
                          onPressed: busy ? null : () => remove(recent[index]),
                        ),
                      ),
                    ),
                  ),
                ),
        ),
        AnimatedBuilder(
          animation: widget.agent,
          builder: (_, _) => deliveryStatus(),
        ),
      ],
    ),
  );
}
