import 'dart:async';
import 'dart:io';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'window_shell.dart';
import 'file_collection.dart';

/// What a drag carries: the side it started on plus full source paths.
class _Drag {
  const _Drag(this.student, this.paths);
  final bool student;
  final List<String> paths;
}

class _Row {
  _Row(this.name, this.dir, this.size, this.modified, this.hidden);
  final String name;
  final bool dir, hidden;
  final int size, modified;
}

/// One browsable folder: the Teacher computer (read by the local agent) or the
/// Student computer (read by the Student over its tools connection).
class _PanelModel extends ChangeNotifier {
  _PanelModel(this.agent, this.student, this.studentId);
  final Agent agent;
  final bool student;
  final String studentId;

  String path = '';
  String sep = '/';
  String parent = '';
  String os = '';
  List<Map<String, dynamic>> roots = const [];
  List<_Row> rows = const [];
  final selected = <String>{};
  String? anchor;
  String? error;
  bool loading = false, hidden = false, truncated = false, disposed = false;
  String sort = 'name';
  bool ascending = true;
  int _generation = 0;

  @override
  void dispose() {
    disposed = true;
    super.dispose();
  }

  String join(String name) =>
      path.endsWith(sep) ? '$path$name' : '$path$sep$name';

  Future<void> load(String target) async {
    final generation = ++_generation;
    loading = true;
    error = null;
    notifyListeners();
    final body = await _fetch(target);
    if (disposed || generation != _generation) return;
    loading = false;
    if (body['error'] != null) {
      error = '${body['error']}';
    } else {
      path = '${body['path']}';
      sep = '${body['sep']}';
      parent = '${body['parent']}';
      os = '${body['os']}';
      truncated = body['truncated'] == true;
      roots = (body['roots'] as List? ?? const [])
          .map((e) => Map<String, dynamic>.from(e as Map))
          .toList();
      rows = (body['entries'] as List? ?? const [])
          .map(
            (e) => _Row(
              '${e['n']}',
              e['d'] == true,
              (e['s'] as num).toInt(),
              (e['m'] as num).toInt(),
              e['h'] == true,
            ),
          )
          .toList();
      selected.clear();
      anchor = null;
      _sortRows();
    }
    notifyListeners();
  }

  Future<Map<String, dynamic>> _fetch(String target) async {
    final started = await agent.request(
      'fs_list',
      input: {
        'side': student ? 'student' : 'local',
        'student': studentId,
        'path': target,
        'hidden': hidden,
      },
    );
    if (started['ok'] != true) return {'error': Agent.failure(started)};
    final id = started['job_id'];
    final deadline = DateTime.now().add(const Duration(seconds: 25));
    while (DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(const Duration(milliseconds: 120));
      final answer = await agent.request('fs_result', input: {'id': id});
      if (answer['ok'] != true) return {'error': Agent.failure(answer)};
      final body = answer['result'];
      if (body is Map && body['pending'] != true) {
        return Map<String, dynamic>.from(body);
      }
    }
    return {'error': 'Máy không phản hồi kịp thời.'};
  }

  void setSort(String key) {
    if (sort == key) {
      ascending = !ascending;
    } else {
      sort = key;
      ascending = true;
    }
    _sortRows();
    notifyListeners();
  }

  void _sortRows() {
    int by(_Row a, _Row b) {
      final c = switch (sort) {
        'size' => a.size.compareTo(b.size),
        'date' => a.modified.compareTo(b.modified),
        _ => a.name.toLowerCase().compareTo(b.name.toLowerCase()),
      };
      return ascending ? c : -c;
    }

    rows = [...rows]
      ..sort((a, b) {
        if (a.dir != b.dir) return a.dir ? -1 : 1;
        return by(a, b);
      });
  }

  void click(_Row row) {
    final ctrl =
        HardwareKeyboard.instance.isControlPressed ||
        HardwareKeyboard.instance.isMetaPressed;
    final shift = HardwareKeyboard.instance.isShiftPressed;
    if (shift && anchor != null) {
      final a = rows.indexWhere((r) => r.name == anchor);
      final b = rows.indexWhere((r) => r.name == row.name);
      if (a >= 0 && b >= 0) {
        selected
          ..clear()
          ..addAll(
            rows
                .sublist(a < b ? a : b, (a < b ? b : a) + 1)
                .where((r) => !r.dir)
                .map((r) => r.name),
          );
      }
    } else if (ctrl) {
      if (!selected.remove(row.name)) selected.add(row.name);
      anchor = row.name;
    } else {
      selected
        ..clear()
        ..add(row.name);
      anchor = row.name;
    }
    notifyListeners();
  }

  List<String> pathsFor(_Row row) {
    final names = selected.contains(row.name) ? selected.toList() : [row.name];
    return names.map(join).toList();
  }
}

class FilesWindow extends StatefulWidget {
  const FilesWindow({
    super.key,
    required this.agent,
    required this.studentId,
    required this.name,
  });
  final Agent agent;
  final String studentId, name;
  @override
  State<FilesWindow> createState() => _FilesWindowState();
}

class _FilesWindowState extends State<FilesWindow> {
  late final _PanelModel local = _PanelModel(
    widget.agent,
    false,
    widget.studentId,
  );
  late final _PanelModel remote = _PanelModel(
    widget.agent,
    true,
    widget.studentId,
  );
  final mine = <String, bool>{}; // job id → pushed to the Student?
  final handled = <String>{};
  Timer? hideTimer;
  String? message;
  bool messageIsError = false;

  Agent get agent => widget.agent;

  bool get online => agent.students.any(
    (s) => s['id'] == widget.studentId && s['online'] == true,
  );

  @override
  void initState() {
    super.initState();
    agent.addListener(onAgent);
    if (agent.connected) {
      first();
    } else {
      agent.addListener(waitForAgent);
    }
  }

  void first() {
    local.load('');
    remote.load('');
  }

  void waitForAgent() {
    if (!agent.connected) return;
    agent.removeListener(waitForAgent);
    first();
  }

  @override
  void dispose() {
    agent.removeListener(onAgent);
    agent.removeListener(waitForAgent);
    hideTimer?.cancel();
    local.dispose();
    remote.dispose();
    super.dispose();
  }

  Map<String, dynamic>? target(Map<String, dynamic> job) {
    for (final t in job['targets'] as List) {
      if (t['id'] == widget.studentId) {
        return Map<String, dynamic>.from(t as Map);
      }
    }
    return null;
  }

  /// Newest transfer this window started, if any.
  Map<String, dynamic>? get current {
    for (final job in agent.fileJobs.reversed) {
      if (mine.containsKey(job['id'])) return job;
    }
    return null;
  }

  void onAgent() {
    if (!mounted) return;
    for (final job in agent.fileJobs) {
      final id = '${job['id']}';
      final t = target(job);
      if (t == null || !mine.containsKey(id) || handled.contains(id)) continue;
      final status = '${t['status']}';
      if (status == 'queued' || status == 'transferring') continue;
      handled.add(id);
      (mine[id]! ? remote : local).load((mine[id]! ? remote : local).path);
      final failed = (t['failed'] as num).toInt();
      final skipped = (t['skipped'] as num).toInt();
      if (status == 'complete') {
        message =
            '${t['files_done']} tệp · đã truyền ${formatBytes(t['bytes_done'] as num)}'
            '${skipped > 0 ? ' · $skipped tệp đã mới nhất' : ''}'
            '${failed > 0 ? ' · $failed tệp không sao chép được: ${(t['notes'] as List).take(2).join('; ')}' : ''}';
        messageIsError = failed > 0;
        hideTimer?.cancel();
        if (failed == 0) {
          hideTimer = Timer(const Duration(seconds: 6), () {
            if (mounted) setState(() => message = null);
          });
        }
      } else if (status == 'cancelled') {
        message = 'Đã hủy truyền tệp';
        messageIsError = false;
      } else {
        message = '${t['error']}';
        messageIsError = true;
      }
    }
    setState(() {});
  }

  Future<void> transfer({
    required bool push,
    required List<String> paths,
    required String dest,
  }) async {
    if (paths.isEmpty || dest.isEmpty) return;
    if (!online) {
      setState(() {
        message = 'Máy học viên đang offline.';
        messageIsError = true;
      });
      return;
    }
    final result = await agent.request(
      'fs_transfer',
      input: {
        'student': widget.studentId,
        'push': push,
        'paths': paths,
        'dest': dest,
      },
    );
    if (!mounted) return;
    if (result['ok'] == true && result['job_id'] != null) {
      mine['${result['job_id']}'] = push;
      hideTimer?.cancel();
      setState(() => message = null);
    } else {
      setState(() {
        message = Agent.failure(result);
        messageIsError = true;
      });
    }
  }

  /// Dropping on a folder row copies into that folder, otherwise into the
  /// folder the target panel is showing.
  void dropOn(_PanelModel panel, _Drag drag, [String? folder]) {
    transfer(
      push: panel.student,
      paths: drag.paths,
      dest: folder ?? panel.path,
    );
  }

  @override
  Widget build(BuildContext context) {
    return WindowShell(
      icon: Icons.folder_copy_outlined,
      title: 'Tệp · ${widget.name}',
      subtitle:
          'Kéo thả hoặc dùng mũi tên để chuyển tệp giữa máy giảng viên và máy học viên.',
      notice: !agent.connected || !online
          ? InlineNotice(
              agent.connected
                  ? '${widget.name} đang offline. Duyệt tệp sẽ tiếp tục khi máy kết nối lại.'
                  : 'Đang kết nối lại với giảng viên…',
              tone: AppTone.warning,
            )
          : null,
      child: Column(
        children: [
          Expanded(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
              child: Row(
                children: [
                  Expanded(
                    child: _Panel(
                      title: 'Máy này (giảng viên)',
                      icon: Icons.laptop_mac,
                      model: local,
                      onDrop: dropOn,
                    ),
                  ),
                  SizedBox(
                    width: 64,
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        _MoveButton(
                          tooltip:
                              'Gửi các tệp đã chọn vào thư mục đang hiện ở bên phải',
                          icon: Icons.arrow_forward,
                          enabled: online && local.selected.isNotEmpty,
                          onPressed: () => transfer(
                            push: true,
                            paths: local.selected.map(local.join).toList(),
                            dest: remote.path,
                          ),
                          model: local,
                        ),
                        const SizedBox(height: 14),
                        _MoveButton(
                          tooltip:
                              'Lấy các tệp đã chọn về thư mục đang hiện ở bên trái',
                          icon: Icons.arrow_back,
                          enabled: online && remote.selected.isNotEmpty,
                          onPressed: () => transfer(
                            push: false,
                            paths: remote.selected.map(remote.join).toList(),
                            dest: local.path,
                          ),
                          model: remote,
                        ),
                      ],
                    ),
                  ),
                  Expanded(
                    child: DropTarget(
                      onDragDone: (details) => transfer(
                        push: true,
                        paths: details.files
                            .map((f) => f.path)
                            .where(
                              (p) =>
                                  FileSystemEntity.typeSync(p) ==
                                  FileSystemEntityType.file,
                            )
                            .toList(),
                        dest: remote.path,
                      ),
                      child: _Panel(
                        title: widget.name,
                        icon: Icons.desktop_windows_outlined,
                        model: remote,
                        onDrop: dropOn,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          _ProgressStrip(
            window: this,
            onDismiss: () => setState(() => message = null),
          ),
        ],
      ),
    );
  }
}

class _MoveButton extends StatelessWidget {
  const _MoveButton({
    required this.tooltip,
    required this.icon,
    required this.enabled,
    required this.onPressed,
    required this.model,
  });
  final String tooltip;
  final IconData icon;
  final bool enabled;
  final VoidCallback onPressed;
  final _PanelModel model;
  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: model,
    builder: (_, _) => IconButton.filledTonal(
      tooltip: tooltip,
      onPressed: enabled && model.selected.isNotEmpty ? onPressed : null,
      icon: Icon(icon, size: 20),
    ),
  );
}

/// Bottom strip: live progress, then a short result message.
class _ProgressStrip extends StatelessWidget {
  const _ProgressStrip({required this.window, required this.onDismiss});
  final _FilesWindowState window;
  final VoidCallback onDismiss;
  @override
  Widget build(BuildContext context) {
    final job = window.current;
    final t = job == null ? null : window.target(job);
    final active = t != null && jobTargetActive(t);
    if (!active && window.message == null) return const SizedBox.shrink();
    if (active) {
      final total = (t['bytes_total'] as num).toDouble();
      final ratio = total > 0
          ? ((t['bytes_done'] as num) / total).clamp(0.0, 1.0)
          : null;
      final push = job!['kind'] == 'push';
      return Container(
        color: Colors.white,
        padding: const EdgeInsets.fromLTRB(14, 8, 8, 8),
        child: Row(
          children: [
            SizedBox(
              width: 18,
              height: 18,
              child: CircularProgressIndicator(
                strokeWidth: 2.4,
                value: t['status'] == 'queued' ? null : ratio,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    t['status'] == 'queued'
                        ? (push ? 'Đang gửi…' : 'Đang chuẩn bị tải về…')
                        : '${push ? 'Đang gửi' : 'Đang tải về'} ${t['files_done']}/${t['files_total']} tệp · '
                              '${formatBytes(t['bytes_done'] as num)} / ${formatBytes(total)}',
                    style: const TextStyle(fontSize: 12.5),
                  ),
                  if ('${t['current']}'.isNotEmpty)
                    Text(
                      '${t['current']}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 11,
                        color: Colors.blueGrey,
                      ),
                    ),
                  const SizedBox(height: 4),
                  LinearProgressIndicator(
                    value: t['status'] == 'queued' ? null : ratio,
                    minHeight: 3,
                  ),
                ],
              ),
            ),
            TextButton(
              onPressed: () =>
                  window.agent.request('fs_cancel', input: {'id': job['id']}),
              child: const Text('Hủy'),
            ),
          ],
        ),
      );
    }
    final bad = window.messageIsError;
    return Container(
      color: bad ? const Color(0xfffdecec) : const Color(0xffe7f4ee),
      padding: const EdgeInsets.fromLTRB(14, 6, 4, 6),
      child: Row(
        children: [
          Icon(
            bad ? Icons.error_outline : Icons.check_circle_outline,
            size: 18,
            color: bad ? const Color(0xffb83b40) : const Color(0xff1f7a4d),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              window.message!,
              style: const TextStyle(fontSize: 12.5),
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          IconButton(
            visualDensity: VisualDensity.compact,
            icon: const Icon(Icons.close, size: 16),
            onPressed: onDismiss,
          ),
        ],
      ),
    );
  }
}

String _date(int seconds) {
  if (seconds <= 0) return '';
  final t = DateTime.fromMillisecondsSinceEpoch(seconds * 1000);
  String two(int v) => v.toString().padLeft(2, '0');
  return '${t.year}-${two(t.month)}-${two(t.day)} ${two(t.hour)}:${two(t.minute)}';
}

class _Panel extends StatefulWidget {
  const _Panel({
    required this.title,
    required this.icon,
    required this.model,
    required this.onDrop,
  });
  final String title;
  final IconData icon;
  final _PanelModel model;
  final void Function(_PanelModel, _Drag, [String?]) onDrop;
  @override
  State<_Panel> createState() => _PanelState();
}

class _PanelState extends State<_Panel> {
  final pathField = TextEditingController();
  bool over = false;
  String? overFolder; // folder under a dragged file
  String? hoverRow; // row under the mouse
  String? focusedDir; // folder last clicked (single click)
  String? flashDir; // brief click pulse
  _PanelModel get model => widget.model;

  @override
  void initState() {
    super.initState();
    model.addListener(sync);
  }

  void sync() {
    if (!mounted) return;
    if (model.path.isNotEmpty && pathField.text != model.path) {
      pathField.text = model.path;
      focusedDir = null;
    }
    if (model.selected.isNotEmpty) focusedDir = null;
    setState(() {});
  }

  @override
  void dispose() {
    model.removeListener(sync);
    pathField.dispose();
    super.dispose();
  }

  Widget header(String label, String key, {double? width, TextAlign? align}) {
    final active = model.sort == key;
    final text = Text(
      '$label${active ? (model.ascending ? ' ▲' : ' ▼') : ''}',
      textAlign: align,
      style: const TextStyle(
        fontSize: 11,
        fontWeight: FontWeight.w600,
        color: Colors.blueGrey,
      ),
    );
    final child = InkWell(onTap: () => model.setSort(key), child: text);
    return width == null
        ? Expanded(child: child)
        : SizedBox(width: width, child: child);
  }

  /// Single click on a folder: focus it with a short pulse (double-click opens).
  void pulse(String name) {
    setState(() {
      focusedDir = name;
      flashDir = name;
    });
    Future<void>.delayed(const Duration(milliseconds: 140), () {
      if (mounted && flashDir == name) setState(() => flashDir = null);
    });
  }

  Widget row(_Row r) {
    final chosen = model.selected.contains(r.name);
    final dropping = overFolder == r.name;
    final pointed = hoverRow == r.name;
    final focused = r.dir && focusedDir == r.name;
    final flashing = flashDir == r.name;
    final body = AnimatedContainer(
      // Hover must follow the mouse instantly; only the click pulse eases in.
      duration: flashing ? const Duration(milliseconds: 50) : Duration.zero,
      decoration: BoxDecoration(
        color: dropping
            ? const Color(0xffbfe0e6)
            : flashing
            ? const Color(0xffa9d3db)
            : chosen
            ? const Color(0xffcfe6ea)
            : focused
            ? const Color(0xffdcecef)
            : pointed
            ? const Color(0xffeaf1f5)
            : Colors.transparent,
        border: Border(
          left: BorderSide(
            width: 3,
            color: chosen || focused || dropping
                ? const Color(0xff156b79)
                : Colors.transparent,
          ),
        ),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      child: Row(
        children: [
          Icon(
            r.dir ? Icons.folder : Icons.insert_drive_file_outlined,
            size: 18,
            color: r.dir ? const Color(0xff3b8ea0) : Colors.blueGrey,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              r.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 12.5,
                color: r.hidden ? Colors.grey : null,
              ),
            ),
          ),
          SizedBox(
            width: 70,
            child: Text(
              r.dir ? '' : formatBytes(r.size),
              textAlign: TextAlign.right,
              style: const TextStyle(fontSize: 11, color: Colors.blueGrey),
            ),
          ),
          SizedBox(
            width: 118,
            child: Text(
              _date(r.modified),
              textAlign: TextAlign.right,
              style: const TextStyle(fontSize: 11, color: Colors.blueGrey),
            ),
          ),
        ],
      ),
    );
    Widget item = GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: r.dir ? () => pulse(r.name) : () => model.click(r),
      onDoubleTap: r.dir ? () => model.load(model.join(r.name)) : null,
      child: body,
    );
    item = MouseRegion(
      cursor: r.dir ? SystemMouseCursors.click : MouseCursor.defer,
      onEnter: (_) => setState(() => hoverRow = r.name),
      onExit: (_) {
        if (hoverRow == r.name) setState(() => hoverRow = null);
      },
      child: item,
    );
    // Only files can be transferred here; folders are only drop targets.
    if (!r.dir) {
      item = Draggable<_Drag>(
        data: _Drag(model.student, model.pathsFor(r)),
        dragAnchorStrategy: pointerDragAnchorStrategy,
        onDragStarted: () {
          if (!model.selected.contains(r.name)) model.click(r);
        },
        feedback: Material(
          color: Colors.transparent,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
            decoration: BoxDecoration(
              color: const Color(0xff156b79),
              borderRadius: BorderRadius.circular(8),
            ),
            child: Text(
              model.selected.length > 1 && model.selected.contains(r.name)
                  ? '${model.selected.length} items'
                  : r.name,
              style: const TextStyle(color: Colors.white, fontSize: 12),
            ),
          ),
        ),
        child: item,
      );
    }
    if (r.dir) {
      final inner = item;
      item = DragTarget<_Drag>(
        onWillAcceptWithDetails: (d) => d.data.student != model.student,
        onMove: (_) {
          if (overFolder != r.name) setState(() => overFolder = r.name);
        },
        onLeave: (_) {
          if (overFolder == r.name) setState(() => overFolder = null);
        },
        onAcceptWithDetails: (d) {
          setState(() => overFolder = null);
          widget.onDrop(model, d.data, model.join(r.name));
        },
        builder: (_, _, _) => inner,
      );
    }
    return item;
  }

  @override
  Widget build(BuildContext context) {
    final up = model.parent;
    return DragTarget<_Drag>(
      onWillAcceptWithDetails: (d) => d.data.student != model.student,
      onMove: (_) {
        if (!over) setState(() => over = true);
      },
      onLeave: (_) => setState(() => over = false),
      onAcceptWithDetails: (d) {
        setState(() => over = false);
        widget.onDrop(model, d.data);
      },
      builder: (_, _, _) => DecoratedBox(
        decoration: BoxDecoration(
          color: Colors.white,
          borderRadius: BorderRadius.circular(10),
          border: Border.all(
            color: over ? const Color(0xff156b79) : const Color(0xffdfe5eb),
            width: over ? 2 : 1,
          ),
        ),
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 8, 8, 2),
              child: Row(
                children: [
                  Icon(widget.icon, size: 17, color: const Color(0xff156b79)),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      widget.title,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontWeight: FontWeight.w600),
                    ),
                  ),
                  PopupMenuButton<String>(
                    tooltip: 'Truy cập nhanh',
                    icon: const Icon(Icons.bookmarks_outlined, size: 18),
                    padding: EdgeInsets.zero,
                    onSelected: model.load,
                    itemBuilder: (_) => [
                      for (final r in model.roots)
                        PopupMenuItem(
                          value: '${r['path']}',
                          child: Text('${r['name']}'),
                        ),
                    ],
                  ),
                  IconButton(
                    tooltip: model.hidden ? 'Ẩn tệp ẩn' : 'Hiện tệp ẩn',
                    visualDensity: VisualDensity.compact,
                    icon: Icon(
                      model.hidden ? Icons.visibility : Icons.visibility_off,
                      size: 18,
                    ),
                    onPressed: () {
                      model.hidden = !model.hidden;
                      model.load(model.path);
                    },
                  ),
                  IconButton(
                    tooltip: 'Làm mới',
                    visualDensity: VisualDensity.compact,
                    icon: const Icon(Icons.refresh, size: 18),
                    onPressed: () => model.load(model.path),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Row(
                children: [
                  IconButton(
                    tooltip: 'Lên thư mục cha',
                    visualDensity: VisualDensity.compact,
                    icon: const Icon(Icons.arrow_upward, size: 18),
                    onPressed: up.isEmpty ? null : () => model.load(up),
                  ),
                  Expanded(
                    child: SizedBox(
                      height: 30,
                      child: TextField(
                        controller: pathField,
                        style: const TextStyle(fontSize: 12),
                        onSubmitted: model.load,
                        decoration: InputDecoration(
                          isDense: true,
                          filled: true,
                          fillColor: const Color(0xfff3f5f9),
                          contentPadding: const EdgeInsets.symmetric(
                            horizontal: 8,
                            vertical: 8,
                          ),
                          border: OutlineInputBorder(
                            borderRadius: BorderRadius.circular(6),
                            borderSide: BorderSide.none,
                          ),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 6, 16, 2),
              child: Row(
                children: [
                  header('Tên', 'name'),
                  header(
                    'Dung lượng',
                    'size',
                    width: 70,
                    align: TextAlign.right,
                  ),
                  header('Sửa đổi', 'date', width: 118, align: TextAlign.right),
                ],
              ),
            ),
            const Divider(height: 1),
            Expanded(
              child: model.error != null
                  ? Center(
                      child: Padding(
                        padding: const EdgeInsets.all(16),
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              model.error!,
                              textAlign: TextAlign.center,
                              style: const TextStyle(color: Color(0xffb83b40)),
                            ),
                            const SizedBox(height: 8),
                            OutlinedButton(
                              onPressed: () => model.load(model.path),
                              child: const Text('Thử lại'),
                            ),
                          ],
                        ),
                      ),
                    )
                  : Stack(
                      children: [
                        if (model.rows.isEmpty && !model.loading)
                          const Center(
                            child: Text(
                              'Thư mục này trống',
                              style: TextStyle(color: Colors.blueGrey),
                            ),
                          )
                        else
                          ListView.builder(
                            itemCount: model.rows.length,
                            itemExtent: 28,
                            itemBuilder: (_, i) => row(model.rows[i]),
                          ),
                        if (model.loading)
                          const Positioned(
                            top: 0,
                            left: 0,
                            right: 0,
                            child: LinearProgressIndicator(minHeight: 2),
                          ),
                      ],
                    ),
            ),
            if (model.truncated)
              const Padding(
                padding: EdgeInsets.all(4),
                child: Text(
                  'Chỉ hiển thị 5000 mục đầu tiên.',
                  style: TextStyle(fontSize: 11, color: Colors.orange),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
