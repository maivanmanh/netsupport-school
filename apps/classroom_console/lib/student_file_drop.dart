import 'icon_tints.dart';
import 'dart:async';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';
import 'package:path/path.dart' as p;

/// One roster item is one recipient, independent of presentation selection.
class StudentFileDrop extends StatefulWidget {
  const StudentFileDrop({
    super.key,
    required this.name,
    required this.ip,
    required this.canSend,
    required this.onSend,
    required this.child,
  });
  final String name, ip;
  final bool Function() canSend;
  final Future<void> Function(List<DropItem>) onSend;
  final Widget child;
  @override
  State<StudentFileDrop> createState() => _StudentFileDropState();
}

class _StudentFileDropState extends State<StudentFileDrop> {
  bool hovering = false, busy = false, sending = false;

  Future<void> drop(List<DropItem> items) async {
    if (busy ||
        items.isEmpty ||
        !widget.canSend() ||
        ModalRoute.of(context)?.isCurrent == false) {
      return;
    }
    setState(() {
      hovering = false;
      busy = true;
    });
    try {
      if (items.length > 32) {
        throw StateError('Drop at most 32 items at a time.');
      }
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Send to this computer?'),
          content: SizedBox(
            width: 380,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  widget.name,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                Text(widget.ip, style: const TextStyle(color: Colors.blueGrey)),
                const SizedBox(height: 16),
                Text(
                  items.length == 1
                      ? p.basename(items.first.path)
                      : '${items.length} files / folders',
                ),
                const SizedBox(height: 8),
                const Text(
                  'Folders will be sent as ZIP files.\nDestination: student Desktop.',
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton.icon(
              onPressed: () => Navigator.pop(context, true),
              icon: const TIcon(Icons.send_outlined, size: 16),
              label: const Text('Send'),
            ),
          ],
        ),
      );
      if (confirmed != true || !mounted) return;
      if (!widget.canSend()) {
        throw StateError(
          'Student is no longer connected or ready to receive files.',
        );
      }
      setState(() => sending = true);
      await widget.onSend(items);
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text('Could not send: $error')));
      }
    } finally {
      if (mounted) {
        setState(() {
          busy = false;
          sending = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) => DropTarget(
    enable: !busy && widget.canSend(),
    onDragEntered: (_) {
      if (ModalRoute.of(context)?.isCurrent != false) {
        setState(() => hovering = true);
      }
    },
    onDragExited: (_) => setState(() => hovering = false),
    onDragDone: (detail) => unawaited(drop(detail.files)),
    child: Stack(
      children: [
        widget.child,
        if (hovering || sending)
          Positioned.fill(
            child: IgnorePointer(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: const Color(
                    0xff156b79,
                  ).withValues(alpha: hovering ? 0.12 : 0.06),
                  borderRadius: BorderRadius.circular(14),
                  border: Border.all(
                    color: const Color(0xff156b79),
                    width: hovering ? 2 : 1,
                  ),
                ),
                child: hovering
                    ? const Center(
                        child: Chip(
                          avatar: TIcon(Icons.file_upload_outlined, size: 16),
                          label: Text('Drop to send…'),
                        ),
                      )
                    : const Align(
                        alignment: Alignment.topCenter,
                        child: LinearProgressIndicator(minHeight: 2),
                      ),
              ),
            ),
          ),
      ],
    ),
  );
}
