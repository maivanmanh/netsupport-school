import 'icon_tints.dart';
import 'package:flutter/material.dart';
import 'app_dialog.dart';

/// Chrome shared by the large child windows (Internet, Applications, VS Code,
/// Activity, Files, Hardware…): a header with icon, title, subtitle and
/// actions, an optional notice strip, the body, and an optional footer bar.
/// It mirrors [AppDialog] so a window and a dialog feel like one product.
class WindowShell extends StatelessWidget {
  const WindowShell({
    super.key,
    required this.title,
    required this.child,
    this.subtitle,
    this.icon,
    this.tone = AppTone.brand,
    this.actions = const [],
    this.notice,
    this.footer,
  });

  final String title;
  final String? subtitle;
  final IconData? icon;
  final AppTone tone;

  /// Controls on the right of the header (buttons, status chips).
  final List<Widget> actions;

  /// A strip under the header, e.g. an [InlineNotice] about the connection.
  final Widget? notice;
  final Widget child;

  /// Action bar along the bottom (primary buttons, progress text).
  final Widget? footer;

  static const background = Color(0xfff3f5f9);

  @override
  Widget build(BuildContext context) => Scaffold(
    backgroundColor: background,
    body: Column(
      children: [
        Container(
          decoration: const BoxDecoration(
            color: Colors.white,
            border: Border(bottom: BorderSide(color: AppDialog.border)),
          ),
          padding: const EdgeInsets.fromLTRB(24, 14, 20, 14),
          child: Row(
            children: [
              if (icon != null) ...[
                Container(
                  width: 40,
                  height: 40,
                  decoration: BoxDecoration(
                    color: tone.color.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: TIcon(icon, size: 21, color: tone.color),
                ),
                const SizedBox(width: 14),
              ],
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 17,
                        fontWeight: FontWeight.w700,
                        height: 1.2,
                        color: AppDialog.ink,
                      ),
                    ),
                    if (subtitle != null)
                      Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: Text(
                          subtitle!,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontSize: 12.5,
                            color: AppDialog.muted,
                          ),
                        ),
                      ),
                  ],
                ),
              ),
              for (final action in actions) ...[
                const SizedBox(width: 10),
                action,
              ],
            ],
          ),
        ),
        if (notice != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 12, 24, 0),
            child: notice,
          ),
        Expanded(child: child),
        if (footer != null)
          Container(
            width: double.infinity,
            decoration: const BoxDecoration(
              color: Colors.white,
              border: Border(top: BorderSide(color: AppDialog.border)),
            ),
            padding: const EdgeInsets.fromLTRB(24, 10, 24, 10),
            child: footer,
          ),
      ],
    ),
  );
}

/// A white content card used inside windows.
class WindowCard extends StatelessWidget {
  const WindowCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(18),
  });
  final Widget child;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) => Card(
    elevation: 0,
    color: Colors.white,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(12),
      side: const BorderSide(color: AppDialog.border),
    ),
    child: Padding(padding: padding, child: child),
  );
}

/// Small coloured count chip, e.g. "Đã áp dụng 3".
class StatusChip extends StatelessWidget {
  const StatusChip(this.label, this.color, {super.key});
  final String label;
  final Color color;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
    decoration: BoxDecoration(
      color: color.withValues(alpha: 0.11),
      borderRadius: BorderRadius.circular(12),
    ),
    child: Text(
      label,
      style: TextStyle(fontSize: 12, fontWeight: FontWeight.w600, color: color),
    ),
  );
}
