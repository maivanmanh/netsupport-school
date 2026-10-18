import 'package:flutter/material.dart';
import 'app_icons.dart';
import 'icon_tints.dart';

/// What a Student is doing right now, as reported while "Theo dõi hoạt động"
/// is on: `{apps: [{app, title, icon?}], url?, tab_title?}`. Null when nothing
/// recent is known (monitoring off, Student offline or not yet reported).
Map? liveActivityFor(Map tools, String studentId) {
  if (tools['activity_monitoring'] != true) return null;
  final all = tools['activity_live'];
  if (all is! Map) return null;
  final summary = all[studentId];
  if (summary is! Map) return null;
  final apps = summary['apps'];
  return apps is List && apps.isNotEmpty ? summary : null;
}

/// The site part of an address for compact display: `docs.rs/crate/x` for
/// `https://www.docs.rs/crate/x`. [pathChars] bounds how much path is kept
/// (0 = host only).
String shortUrl(String url, {int pathChars = 0}) {
  final uri = Uri.tryParse(url);
  if (uri == null ||
      uri.host.isEmpty ||
      !(uri.scheme == 'http' || uri.scheme == 'https')) {
    return url;
  }
  final host = uri.host.startsWith('www.') ? uri.host.substring(4) : uri.host;
  if (pathChars <= 0) return host;
  final rest =
      '${uri.path == '/' ? '' : uri.path}${uri.hasQuery ? '?${uri.query}' : ''}';
  return rest.length <= pathChars
      ? '$host$rest'
      : '$host${rest.substring(0, pathChars)}…';
}

/// Hosts of AI / chatbot websites the Student has open in any browser tab
/// (empty when none, or when the Teacher's AI list does not match).
List<String> liveAiHosts(Map? summary) => [
  for (final h in (summary?['ai'] as List? ?? const [])) '$h',
];

/// Where the information goes: a grid tile has little room, a list row more,
/// a compact row is one line.
enum LiveLayout { grid, list, compact }

/// What a Student is doing, laid out for the room available.
///
/// Elements, in order of importance: AI-website warning (always first when
/// present), the page a browser is showing, and up to five application icons.
/// Each layout lets any subset of them look complete rather than patched.
class LiveActivity extends StatelessWidget {
  const LiveActivity({super.key, required this.summary, required this.layout});
  final Map summary;
  final LiveLayout layout;

  double get _icon => switch (layout) {
    LiveLayout.grid => 18,
    LiveLayout.list => 24,
    LiveLayout.compact => 20,
  };

  List<Widget> _apps() {
    final apps = (summary['apps'] as List).cast<Map>();
    return [
      for (final entry in apps)
        Padding(
          padding: const EdgeInsets.only(right: 4),
          child: Tooltip(
            message:
                '${lookAppUp('${entry['app']}').name}'
                '${'${entry['title'] ?? ''}'.isEmpty ? '' : ' — ${entry['title']}'}',
            child: AppBadge(
              app: '${entry['app']}',
              size: _icon,
              icon: entry['icon'] as String?,
            ),
          ),
        ),
    ];
  }

  String? get _url {
    final url = summary['url'] as String?;
    return url == null || url.isEmpty ? null : url;
  }

  /// The page of the browser, unless it is just the AI site already flagged.
  Widget? _site({required bool flagged}) {
    final url = _url;
    if (url == null) return null;
    final host = shortUrl(url);
    if (flagged &&
        liveAiHosts(summary).any((h) => host.endsWith(h) || h.endsWith(host))) {
      return null; // the AI chip already names this site
    }
    final inFront = summary['url_in_front'] != false;
    final title = '${summary['tab_title'] ?? ''}';
    return Tooltip(
      message:
          '${title.isEmpty ? '' : '$title\n'}$url'
          '${inFront ? '' : '\n(trình duyệt đang chạy nền)'}',
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Opacity(
            opacity: inFront ? 1 : 0.7,
            child: SiteIcon(url: url, size: _icon - 3),
          ),
          const SizedBox(width: 5),
          Flexible(
            child: Text(
              shortUrl(
                url,
                pathChars: switch (layout) {
                  LiveLayout.grid => 0,
                  LiveLayout.list => 40,
                  LiveLayout.compact => 24,
                },
              ),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: layout == LiveLayout.list ? 12 : 11,
                color: inFront
                    ? const Color(0xff3f5361)
                    : const Color(0xff7b8b98),
              ),
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final ai = liveAiHosts(summary);
    final chip = ai.isEmpty
        ? null
        : AiChip(hosts: ai, compact: layout == LiveLayout.compact);
    switch (layout) {
      case LiveLayout.grid:
        // Icons, then the AI warning (or the page); the page drops to a second
        // line only when both exist.
        final site = _site(flagged: ai.isNotEmpty);
        return Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              height: _icon + 2,
              child: Row(
                children: [
                  ..._apps(),
                  if (chip != null)
                    Expanded(
                      child: Align(
                        alignment: Alignment.centerRight,
                        child: chip,
                      ),
                    )
                  else if (site != null) ...[
                    const _Divider(),
                    Flexible(child: site),
                  ],
                ],
              ),
            ),
            if (chip != null && site != null)
              Padding(
                padding: const EdgeInsets.only(top: 3),
                child: SizedBox(height: _icon - 2, child: site),
              ),
          ],
        );
      case LiveLayout.list:
        final site = _site(flagged: false);
        return Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: 6,
              runSpacing: 4,
              children: [..._apps(), ?chip],
            ),
            if (site != null) ...[const SizedBox(height: 4), site],
          ],
        );
      case LiveLayout.compact:
        final site = _site(flagged: false);
        return LayoutBuilder(
          builder: (context, box) {
            // Narrow rows keep the AI warning (shortened) and as many icons as
            // fit; the page address gets whatever is left.
            final narrow = box.maxWidth < 330;
            final warning = ai.isEmpty
                ? null
                : AiChip(hosts: ai, compact: true, mini: narrow);
            final reserved = (warning == null ? 0 : (narrow ? 52 : 200)) + 8;
            final apps = _apps();
            final fit = ((box.maxWidth - reserved) / (_icon + 4)).floor().clamp(
              0,
              apps.length,
            );
            final shown = apps
                .take(fit == 0 && warning == null ? 1 : fit)
                .toList();
            final roomForSite =
                box.maxWidth - reserved - shown.length * (_icon + 4) > 70;
            return ClipRect(
              child: Row(
                children: [
                  ...shown,
                  if (site != null && roomForSite) ...[
                    const _Divider(),
                    Expanded(child: site),
                  ] else
                    const Spacer(),
                  ?warning,
                ],
              ),
            );
          },
        );
    }
  }
}

class _Divider extends StatelessWidget {
  const _Divider();
  @override
  Widget build(BuildContext context) => Container(
    width: 1,
    height: 14,
    margin: const EdgeInsets.symmetric(horizontal: 6),
    color: const Color(0xffd5dce3),
  );
}

/// "This Student has an AI / chatbot website open" - deliberately the loudest
/// thing on a tile, in a colour that is neither the recording red nor a status dot.
class AiChip extends StatelessWidget {
  const AiChip({
    super.key,
    required this.hosts,
    this.compact = false,
    this.mini = false,
  });
  final List<String> hosts;
  final bool compact;

  /// Just the icon and "AI": for rows too narrow for a site name.
  final bool mini;

  static const color = Color(0xffc2410c);

  @override
  Widget build(BuildContext context) {
    final first = hosts.first;
    final more = hosts.length - 1;
    return Tooltip(
      message: 'Đang mở website AI: ${hosts.join(', ')}',
      child: Container(
        constraints: BoxConstraints(maxWidth: compact ? 190 : 220),
        padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
        decoration: BoxDecoration(
          color: const Color(0x1fc2410c),
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: const Color(0x66c2410c)),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.smart_toy, size: 13, color: color),
            const SizedBox(width: 4),
            if (!mini) ...[
              // The site's own icon, fetched and cached by the Teacher.
              SiteIcon(url: 'https://$first', size: 13),
              const SizedBox(width: 4),
            ],
            Flexible(
              child: Text(
                mini ? 'AI' : 'AI · $first${more > 0 ? '  +$more' : ''}',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w700,
                  color: color,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A red dot that pulses while a recording is running, so it reads as live.
class BlinkingRecordDot extends StatefulWidget {
  /// Widget tests that wait for the UI to settle turn the pulse off, since a
  /// repeating animation never settles.
  @visibleForTesting
  static bool animated = true;

  const BlinkingRecordDot({
    super.key,
    this.size = 10,
    this.color = const Color(0xffd32f2f),
  });
  final double size;
  final Color color;
  @override
  State<BlinkingRecordDot> createState() => _BlinkingRecordDotState();
}

class _BlinkingRecordDotState extends State<BlinkingRecordDot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 900),
    );
    if (BlinkingRecordDot.animated) _controller.repeat(reverse: true);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => FadeTransition(
    opacity:
        BlinkingRecordDot.animated && !MediaQuery.disableAnimationsOf(context)
        ? Tween<double>(begin: 0.25, end: 1).animate(_controller)
        : const AlwaysStoppedAnimation(1.0),
    child: TIcon(
      Icons.fiber_manual_record,
      size: widget.size,
      color: widget.color,
    ),
  );
}
