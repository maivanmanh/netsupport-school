import 'package:flutter/material.dart';
import 'agent.dart';

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
    if (value.isEmpty) throw const FormatException('Enter a website address.');
    if (!value.contains('://')) value = 'https://$value';
    final uri = Uri.tryParse(value);
    if (value.length > 4096 ||
        value.contains(RegExp(r'[\s\x00-\x1f\x7f]')) ||
        uri == null ||
        !['http', 'https'].contains(uri.scheme) ||
        uri.host.isEmpty ||
        uri.userInfo.isNotEmpty) {
      throw const FormatException(
        'Use a valid http:// or https:// website address.',
      );
    }
    return uri.toString();
  }

  Future<void> persist(List<String> values) async {
    if (widget.agent.dataDirectory == null) {
      throw StateError('Teacher storage is unavailable.');
    }
    // Await the flushed on-disk settings update before dispatching the request.
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
      if (ids.isEmpty) throw StateError('No target computers are online.');
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
      if (mounted) setState(() => error = 'Could not save recent websites: $e');
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
          labelText: 'Website URL',
          hintText: 'https://example.com',
          prefixIcon: const Icon(Icons.link),
          helperText: 'Type an address or choose a saved suggestion.',
          border: OutlineInputBorder(borderRadius: BorderRadius.circular(10)),
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
                  leading: const Icon(Icons.history, size: 18),
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
          Icon(
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
                      ? 'Opening… $complete/${targets.length}'
                      : 'Opened on $complete computer${complete == 1 ? '' : 's'}${failed.isEmpty ? '' : ' · ${failed.length} failed'}',
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
  Widget build(BuildContext context) => AlertDialog(
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
    title: Row(
      children: [
        Container(
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: const Color(0xffe4f1f2),
            borderRadius: BorderRadius.circular(12),
          ),
          child: const Icon(Icons.language, color: Color(0xff237477)),
        ),
        const SizedBox(width: 14),
        const Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Open website'),
              SizedBox(height: 4),
              Text(
                'In each Student’s default browser',
                style: TextStyle(fontSize: 13, color: Colors.blueGrey),
              ),
            ],
          ),
        ),
      ],
    ),
    content: SizedBox(
      width: 590,
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
              decoration: BoxDecoration(
                color: const Color(0xfff2f5f8),
                borderRadius: BorderRadius.circular(8),
              ),
              child: Row(
                children: [
                  const Icon(Icons.computer_outlined, size: 18),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Send to ${widget.audience}',
                      style: const TextStyle(fontSize: 13),
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 20),
            urlInput(),
            if (error != null)
              Padding(
                padding: const EdgeInsets.only(top: 10),
                child: Text(
                  error!,
                  style: const TextStyle(color: Color(0xffb83b40)),
                ),
              ),
            const SizedBox(height: 24),
            const Text(
              'RECENT WEBSITES',
              style: TextStyle(
                fontSize: 11,
                letterSpacing: 1.2,
                fontWeight: FontWeight.w700,
                color: Colors.blueGrey,
              ),
            ),
            const SizedBox(height: 4),
            const Text(
              'Click an address to open it on these computers.',
              style: TextStyle(fontSize: 12, color: Colors.blueGrey),
            ),
            const SizedBox(height: 10),
            if (recent.isEmpty)
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(24),
                decoration: BoxDecoration(
                  border: Border.all(color: const Color(0xffe1e7ed)),
                  borderRadius: BorderRadius.circular(10),
                ),
                child: const Column(
                  children: [
                    Icon(Icons.history, color: Colors.blueGrey),
                    SizedBox(height: 8),
                    Text(
                      'Your recent websites will appear here.',
                      style: TextStyle(color: Colors.blueGrey),
                    ),
                  ],
                ),
              )
            else
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 240),
                child: ListView.separated(
                  shrinkWrap: true,
                  itemCount: recent.length,
                  separatorBuilder: (_, _) => const Divider(height: 1),
                  itemBuilder: (_, index) => ListTile(
                    contentPadding: const EdgeInsets.symmetric(horizontal: 4),
                    dense: true,
                    leading: const Icon(
                      Icons.public,
                      size: 20,
                      color: Color(0xff237477),
                    ),
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
                      tooltip: 'Remove from recent websites',
                      icon: const Icon(Icons.close, size: 17),
                      onPressed: busy ? null : () => remove(recent[index]),
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
      ),
    ),
    actionsPadding: const EdgeInsets.fromLTRB(24, 8, 24, 20),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Close'),
      ),
      FilledButton.icon(
        onPressed: busy ? null : () => open(),
        icon: busy
            ? const SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : const Icon(Icons.open_in_browser, size: 19),
        label: Text(busy ? 'Opening…' : 'Open'),
      ),
    ],
  );
}
