import 'package:flutter/material.dart';

/// Shared look of every Teacher dialog: a header (icon, title, optional
/// subtitle, close), a scrolling body and a footer holding the actions.
/// Dialogs differ only in what they put in the body and the footer, so the
/// spacing, colours and button hierarchy stay identical everywhere.
class AppDialog extends StatelessWidget {
  const AppDialog({
    super.key,
    required this.title,
    required this.child,
    this.icon,
    this.subtitle,
    this.actions = const [],
    this.footerLeading,
    this.width = 520,
    this.height,
    this.scrollable = true,
    this.onClose,
    this.bodyPadding = const EdgeInsets.fromLTRB(24, 18, 24, 20),
    this.tone = AppTone.brand,
  });

  final String title;
  final String? subtitle;
  final IconData? icon;
  final Widget child;

  /// Footer buttons, left to right. Build them with [AppDialog.primary],
  /// [AppDialog.secondary] and [AppDialog.danger] so the hierarchy is uniform.
  final List<Widget> actions;

  /// Left side of the footer: progress text, a status, a secondary link.
  final Widget? footerLeading;
  final double width;

  /// A fixed body height for list-style dialogs; otherwise the body hugs its
  /// content and scrolls only when the window is too short.
  final double? height;
  final bool scrollable;

  /// The header close button; defaults to popping the dialog.
  final VoidCallback? onClose;
  final EdgeInsets bodyPadding;
  final AppTone tone;

  static const border = Color(0xffe3e9ee);
  static const footerFill = Color(0xfff6f8fa);
  static const ink = Color(0xff1b2a36);
  static const muted = Color(0xff5f7384);

  /// The main action of the dialog (filled).
  static Widget primary({
    required String label,
    required VoidCallback? onPressed,
    IconData? icon,
    bool busy = false,
    bool destructive = false,
  }) {
    final style = FilledButton.styleFrom(
      minimumSize: const Size(0, 38),
      padding: const EdgeInsets.symmetric(horizontal: 18),
      textStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
      backgroundColor: destructive ? const Color(0xffb83b40) : null,
    );
    if (icon == null && !busy) {
      return FilledButton(
        style: style,
        onPressed: onPressed,
        child: Text(label),
      );
    }
    return FilledButton.icon(
      style: style,
      onPressed: busy ? null : onPressed,
      icon: busy
          ? const SizedBox(
              width: 15,
              height: 15,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          : Icon(icon, size: 17),
      label: Text(label),
    );
  }

  /// A supporting action (outlined), e.g. Edit, Add.
  static Widget secondary({
    required String label,
    required VoidCallback? onPressed,
    IconData? icon,
  }) => OutlinedButton.icon(
    style: OutlinedButton.styleFrom(
      minimumSize: const Size(0, 38),
      padding: const EdgeInsets.symmetric(horizontal: 16),
      textStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w500),
      side: const BorderSide(color: Color(0xffc9d3db)),
    ),
    onPressed: onPressed,
    icon: icon == null ? const SizedBox.shrink() : Icon(icon, size: 17),
    label: Text(label),
  );

  /// Dismiss / cancel (text only).
  static Widget cancel(
    BuildContext context, {
    String label = 'Hủy',
    VoidCallback? onPressed,
    bool enabled = true,
  }) => TextButton(
    style: TextButton.styleFrom(
      minimumSize: const Size(0, 38),
      padding: const EdgeInsets.symmetric(horizontal: 16),
      textStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w500),
      foregroundColor: muted,
    ),
    onPressed: enabled ? (onPressed ?? () => Navigator.pop(context)) : null,
    child: Text(label),
  );

  @override
  Widget build(BuildContext context) {
    final screen = MediaQuery.sizeOf(context);
    final color = tone.color;
    final body = Padding(padding: bodyPadding, child: child);
    return Dialog(
      backgroundColor: Colors.white,
      surfaceTintColor: Colors.transparent,
      elevation: 10,
      shadowColor: const Color(0x33102030),
      clipBehavior: Clip.antiAlias,
      insetPadding: const EdgeInsets.symmetric(horizontal: 24, vertical: 24),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: screen.height * 0.9),
        child: SizedBox(
          width: width.clamp(280, screen.width - 48),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(24, 18, 12, 16),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    if (icon != null) ...[
                      Container(
                        width: 40,
                        height: 40,
                        decoration: BoxDecoration(
                          color: color.withValues(alpha: 0.12),
                          borderRadius: BorderRadius.circular(10),
                        ),
                        child: Icon(icon, size: 21, color: color),
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
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontSize: 17,
                              fontWeight: FontWeight.w700,
                              height: 1.2,
                              color: ink,
                            ),
                          ),
                          if (subtitle != null) ...[
                            const SizedBox(height: 3),
                            Text(
                              subtitle!,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                fontSize: 12.5,
                                height: 1.3,
                                color: muted,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                    IconButton(
                      tooltip: 'Đóng',
                      visualDensity: VisualDensity.compact,
                      splashRadius: 18,
                      onPressed: onClose ?? () => Navigator.maybePop(context),
                      icon: const Icon(Icons.close, size: 19, color: muted),
                    ),
                  ],
                ),
              ),
              const Divider(height: 1, thickness: 1, color: border),
              Flexible(
                child: height != null
                    ? SizedBox(height: height, child: body)
                    : scrollable
                    ? SingleChildScrollView(child: body)
                    : body,
              ),
              if (actions.isNotEmpty || footerLeading != null)
                Container(
                  decoration: const BoxDecoration(
                    color: footerFill,
                    border: Border(top: BorderSide(color: border)),
                  ),
                  padding: const EdgeInsets.fromLTRB(24, 12, 24, 12),
                  child: Row(
                    children: [
                      if (footerLeading != null)
                        Expanded(child: footerLeading!)
                      else
                        const Spacer(),
                      for (var i = 0; i < actions.length; i++) ...[
                        if (i > 0 || footerLeading != null)
                          const SizedBox(width: 10),
                        actions[i],
                      ],
                    ],
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

enum AppTone {
  brand(Color(0xff156b79)),
  info(Color(0xff2d6cb5)),
  warning(Color(0xffc77d12)),
  danger(Color(0xffb83b40)),
  success(Color(0xff1f7a4d));

  const AppTone(this.color);
  final Color color;
}

/// A titled group inside a dialog body.
class DialogSection extends StatelessWidget {
  const DialogSection({
    super.key,
    required this.title,
    required this.child,
    this.hint,
    this.padBottom = 18,
  });
  final String title;
  final String? hint;
  final Widget child;
  final double padBottom;

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.only(bottom: padBottom),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          title,
          style: const TextStyle(
            fontSize: 12.5,
            fontWeight: FontWeight.w700,
            color: AppDialog.ink,
          ),
        ),
        if (hint != null) ...[
          const SizedBox(height: 2),
          Text(
            hint!,
            style: const TextStyle(
              fontSize: 12,
              color: AppDialog.muted,
              height: 1.35,
            ),
          ),
        ],
        const SizedBox(height: 8),
        child,
      ],
    ),
  );
}

/// An inline status line (error, warning, info) for a dialog body.
class InlineNotice extends StatelessWidget {
  const InlineNotice(this.message, {super.key, this.tone = AppTone.danger});
  final String message;
  final AppTone tone;

  @override
  Widget build(BuildContext context) => Container(
    width: double.infinity,
    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
    decoration: BoxDecoration(
      color: tone.color.withValues(alpha: 0.09),
      borderRadius: BorderRadius.circular(8),
      border: Border.all(color: tone.color.withValues(alpha: 0.28)),
    ),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(
          tone == AppTone.danger
              ? Icons.error_outline
              : tone == AppTone.warning
              ? Icons.warning_amber_rounded
              : tone == AppTone.success
              ? Icons.check_circle_outline
              : Icons.info_outline,
          size: 17,
          color: tone.color,
        ),
        const SizedBox(width: 9),
        Expanded(
          child: Text(
            message,
            style: TextStyle(fontSize: 12.5, height: 1.35, color: tone.color),
          ),
        ),
      ],
    ),
  );
}

/// Centered empty state for lists inside dialogs.
class DialogEmpty extends StatelessWidget {
  const DialogEmpty({
    super.key,
    required this.icon,
    required this.title,
    this.message,
  });
  final IconData icon;
  final String title;
  final String? message;

  @override
  Widget build(BuildContext context) => Center(
    child: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 38, color: const Color(0xffa9b8c4)),
          const SizedBox(height: 10),
          Text(
            title,
            textAlign: TextAlign.center,
            style: const TextStyle(
              fontSize: 14,
              fontWeight: FontWeight.w600,
              color: AppDialog.ink,
            ),
          ),
          if (message != null) ...[
            const SizedBox(height: 4),
            Text(
              message!,
              textAlign: TextAlign.center,
              style: const TextStyle(
                fontSize: 12.5,
                color: AppDialog.muted,
                height: 1.4,
              ),
            ),
          ],
        ],
      ),
    ),
  );
}

/// A bordered panel used for lists inside a dialog body.
class DialogPanel extends StatelessWidget {
  const DialogPanel({super.key, required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: BoxDecoration(
      color: Colors.white,
      border: Border.all(color: AppDialog.border),
      borderRadius: BorderRadius.circular(10),
    ),
    child: ClipRRect(borderRadius: BorderRadius.circular(10), child: child),
  );
}

/// Confirmation dialog with one clear consequence line.
Future<bool> confirmDialog(
  BuildContext context, {
  required String title,
  required String message,
  required String confirmLabel,
  IconData icon = Icons.help_outline,
  bool destructive = false,
  String cancelLabel = 'Hủy',
}) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (context) => AppDialog(
      title: title,
      icon: icon,
      tone: destructive ? AppTone.danger : AppTone.brand,
      width: 440,
      bodyPadding: const EdgeInsets.fromLTRB(24, 16, 24, 20),
      onClose: () => Navigator.pop(context, false),
      actions: [
        AppDialog.cancel(
          context,
          label: cancelLabel,
          onPressed: () => Navigator.pop(context, false),
        ),
        AppDialog.primary(
          label: confirmLabel,
          destructive: destructive,
          onPressed: () => Navigator.pop(context, true),
        ),
      ],
      child: Text(
        message,
        style: const TextStyle(
          fontSize: 13.5,
          height: 1.45,
          color: AppDialog.ink,
        ),
      ),
    ),
  );
  return result == true;
}
