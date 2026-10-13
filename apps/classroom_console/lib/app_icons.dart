import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';

/// A curated icon + colour + friendly name for common Windows programs,
/// looked up by process (executable) name. Real per-exe icons aren't
/// extracted (would need Windows-side icon extraction); this is a
/// recognisable, consistent stand-in set covering what a classroom actually
/// sees, with a neutral fallback for anything else.
class AppLook {
  const AppLook(this.name, this.icon, this.color);
  final String name;
  final IconData icon;
  final Color color;
}

const _edge = Color(0xff0f8e4f);
const _chrome = Color(0xff4285f4);
const _office = Color(0xff2b579a);
const _excel = Color(0xff1d6f42);
const _ppt = Color(0xffb7472a);
const _code = Color(0xff2496ed);
const _terminal = Color(0xff2b2b2b);
const _pdf = Color(0xffd32f2f);
const _mail = Color(0xff0078d4);
const _meet = Color(0xff5b5fc7);

final Map<String, AppLook> _apps = {
  'msedge.exe': const AppLook('Edge', Icons.language, _edge),
  'chrome.exe': const AppLook('Chrome', Icons.language, _chrome),
  'firefox.exe': const AppLook('Firefox', Icons.language, Color(0xffff7139)),
  'brave.exe': const AppLook('Brave', Icons.language, Color(0xfffb542b)),
  'opera.exe': const AppLook('Opera', Icons.language, Color(0xffff1b2d)),
  'notepad.exe': const AppLook('Notepad', Icons.notes, Colors.blueGrey),
  'notepad++.exe': AppLook('Notepad++', Icons.notes, Colors.teal.shade700),
  'winword.exe': const AppLook('Word', Icons.description, _office),
  'excel.exe': const AppLook('Excel', Icons.grid_on, _excel),
  'powerpnt.exe': const AppLook('PowerPoint', Icons.slideshow, _ppt),
  'onenote.exe': const AppLook('OneNote', Icons.menu_book, Color(0xff7719aa)),
  'outlook.exe': const AppLook('Outlook', Icons.mail, _mail),
  'teams.exe': const AppLook('Teams', Icons.groups, _meet),
  'ms-teams.exe': const AppLook('Teams', Icons.groups, _meet),
  'zoom.exe': const AppLook('Zoom', Icons.videocam, Color(0xff2d8cff)),
  'code.exe': const AppLook('VS Code', Icons.code, _code),
  'devenv.exe': const AppLook('Visual Studio', Icons.code, Color(0xff5c2d91)),
  'pycharm64.exe': const AppLook('PyCharm', Icons.code, Color(0xff21d789)),
  'idea64.exe': const AppLook('IntelliJ IDEA', Icons.code, Color(0xfffe315d)),
  'windowsterminal.exe': const AppLook('Terminal', Icons.terminal, _terminal),
  'cmd.exe': const AppLook('Command Prompt', Icons.terminal, _terminal),
  'powershell.exe': const AppLook(
    'PowerShell',
    Icons.terminal,
    Color(0xff012456),
  ),
  'pwsh.exe': const AppLook('PowerShell', Icons.terminal, Color(0xff012456)),
  'acrobat.exe': const AppLook('Acrobat', Icons.picture_as_pdf, _pdf),
  'acrord32.exe': const AppLook('Acrobat Reader', Icons.picture_as_pdf, _pdf),
  'explorer.exe': const AppLook(
    'File Explorer',
    Icons.folder,
    Color(0xffffc107),
  ),
  'calc.exe': const AppLook('Calculator', Icons.calculate, Colors.indigo),
  'mspaint.exe': const AppLook('Paint', Icons.palette, Colors.deepOrange),
  'snippingtool.exe': const AppLook(
    'Snipping Tool',
    Icons.content_cut,
    Colors.purple,
  ),
  'spotify.exe': const AppLook('Spotify', Icons.music_note, Color(0xff1db954)),
  'discord.exe': const AppLook('Discord', Icons.chat, Color(0xff5865f2)),
  'telegram.exe': const AppLook('Telegram', Icons.send, Color(0xff26a5e4)),
  'vlc.exe': const AppLook('VLC', Icons.play_circle_fill, Color(0xffff8800)),
  'photoshop.exe': const AppLook('Photoshop', Icons.image, Color(0xff31a8ff)),
  'steam.exe': const AppLook('Steam', Icons.sports_esports, Colors.blueGrey),
  'settingsync.exe': const AppLook('Settings', Icons.settings, Colors.blueGrey),
  'systemsettings.exe': const AppLook(
    'Settings',
    Icons.settings,
    Colors.blueGrey,
  ),
};

/// Best-effort name + icon + colour for `processName` (e.g. `chrome.exe`).
AppLook lookAppUp(String processName) {
  final known = _apps[processName.toLowerCase()];
  if (known != null) return known;
  final base = processName.replaceAll(
    RegExp(r'\.exe$', caseSensitive: false),
    '',
  );
  final pretty = base.isEmpty
      ? 'App'
      : base[0].toUpperCase() + base.substring(1);
  return AppLook(pretty, Icons.window_outlined, Colors.blueGrey);
}

/// A round badge with the app's icon/colour, consistent across the UI.
class AppBadge extends StatelessWidget {
  const AppBadge({super.key, required this.app, this.size = 28, this.icon});
  final String app;
  final double size;

  /// The program's real icon (base64 PNG) from the Student, when available;
  /// otherwise the curated stand-in glyph.
  final String? icon;
  @override
  Widget build(BuildContext context) {
    final look = lookAppUp(app);
    final real = icon;
    if (real != null && real.isNotEmpty) {
      try {
        return SizedBox(
          width: size,
          height: size,
          child: Image.memory(
            base64Decode(real),
            fit: BoxFit.contain,
            filterQuality: FilterQuality.medium,
            gaplessPlayback: true,
            errorBuilder: (_, _, _) => _fallback(look),
          ),
        );
      } catch (_) {}
    }
    return _fallback(look);
  }

  Widget _fallback(AppLook look) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: look.color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(size * 0.3),
      ),
      child: Icon(look.icon, size: size * 0.6, color: look.color),
    );
  }
}

/// Fetches site icons from the Teacher's own network connection, one lookup
/// per host for the app's lifetime (a failure is retried after a while). The
/// Student only has to report the address.
class FaviconCache {
  FaviconCache._();
  static final FaviconCache instance = FaviconCache._();
  static const _retryAfter = Duration(minutes: 5);
  static const _maxBytes = 256 * 1024;

  final Map<String, Future<Uint8List?>> _byHost = {};
  final Map<String, DateTime> _failedAt = {};

  /// The icon for [url]'s site, or null if none could be fetched.
  Future<Uint8List?> forUrl(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null ||
        !(uri.scheme == 'http' || uri.scheme == 'https') ||
        uri.host.isEmpty) {
      return Future.value(null);
    }
    final key = '${uri.scheme}://${uri.authority}';
    final failed = _failedAt[key];
    if (failed != null && DateTime.now().difference(failed) > _retryAfter) {
      _byHost.remove(key);
      _failedAt.remove(key);
    }
    return _byHost.putIfAbsent(key, () async {
      final icon = await _fetch(uri);
      if (icon == null) _failedAt[key] = DateTime.now();
      return icon;
    });
  }

  Future<Uint8List?> _fetch(Uri page) async {
    final origin = Uri(
      scheme: page.scheme,
      host: page.host,
      port: page.hasPort ? page.port : null,
    );
    final candidates = <Uri>[];
    final html = await _get(origin.replace(path: '/'), maxBytes: 200 * 1024);
    if (html != null) {
      candidates.addAll(
        _iconLinks(utf8.decode(html, allowMalformed: true), origin),
      );
    }
    candidates.add(origin.replace(path: '/favicon.ico'));
    for (final candidate in candidates.take(4)) {
      final bytes = await _get(candidate, maxBytes: _maxBytes);
      if (bytes != null && await _decodes(bytes)) return bytes;
    }
    // Last resort for sites that block direct requests.
    final fallback = await _get(
      Uri.https('www.google.com', '/s2/favicons', {
        'domain': page.host,
        'sz': '64',
      }),
      maxBytes: _maxBytes,
    );
    if (fallback != null && await _decodes(fallback)) return fallback;
    return null;
  }

  /// `<link rel="...icon..." href="...">` targets, best first: larger and
  /// PNG icons ahead of tiny/ICO ones.
  static List<Uri> _iconLinks(String html, Uri base) {
    final found = <(int, Uri)>[];
    for (final m in RegExp(
      r'<link\b[^>]*>',
      caseSensitive: false,
    ).allMatches(html)) {
      final tag = m.group(0)!;
      String? attr(String name) => RegExp(
        '$name\\s*=\\s*(?:"([^"]*)"|\'([^\']*)\'|([^\\s>]+))',
        caseSensitive: false,
      ).firstMatch(tag)?.let((x) => x.group(1) ?? x.group(2) ?? x.group(3));
      final rel = (attr('rel') ?? '').toLowerCase();
      if (!rel
          .split(RegExp(r'\s+'))
          .any(
            (r) => r == 'icon' || r == 'apple-touch-icon' || r == 'shortcut',
          )) {
        continue;
      }
      final href = attr('href');
      if (href == null || href.isEmpty || href.startsWith('data:')) continue;
      final uri = base.resolve(href);
      if (uri.scheme != 'http' && uri.scheme != 'https') continue;
      final size = int.tryParse((attr('sizes') ?? '').split('x').first) ?? 0;
      final isSvg =
          href.toLowerCase().contains('.svg') ||
          (attr('type') ?? '').contains('svg');
      if (isSvg) continue; // Flutter cannot decode SVG bitmaps
      final score =
          size.clamp(0, 256) + (href.toLowerCase().contains('.png') ? 40 : 0);
      found.add((score, uri));
    }
    found.sort((a, b) => b.$1.compareTo(a.$1));
    return [for (final (_, uri) in found) uri];
  }

  Future<bool> _decodes(Uint8List bytes) async {
    try {
      final codec = await ui.instantiateImageCodec(bytes, targetWidth: 64);
      final frame = await codec.getNextFrame();
      frame.image.dispose();
      codec.dispose();
      return true;
    } catch (_) {
      return false;
    }
  }

  Future<Uint8List?> _get(Uri uri, {required int maxBytes}) async {
    final client = HttpClient()
      ..connectionTimeout = const Duration(seconds: 4)
      ..userAgent = 'Mozilla/5.0 ClassroomTeacher';
    try {
      final request = await client
          .getUrl(uri)
          .timeout(const Duration(seconds: 5));
      final response = await request.close().timeout(
        const Duration(seconds: 5),
      );
      if (response.statusCode != 200) {
        await response.drain<void>();
        return null;
      }
      final out = BytesBuilder(copy: false);
      await for (final chunk in response.timeout(const Duration(seconds: 6))) {
        out.add(chunk);
        if (out.length > maxBytes) return out.takeBytes().sublist(0, maxBytes);
      }
      return out.isEmpty ? null : out.takeBytes();
    } catch (_) {
      return null;
    } finally {
      client.close(force: true);
    }
  }
}

extension<T> on T {
  R let<R>(R Function(T) f) => f(this);
}

/// A site icon for a browser tab: the favicon the Student reported if any,
/// otherwise one the Teacher fetches itself from [url], otherwise a globe.
class SiteIcon extends StatefulWidget {
  const SiteIcon({super.key, this.favicon, this.url, this.size = 18});
  final String? favicon, url;
  final double size;
  @override
  State<SiteIcon> createState() => _SiteIconState();
}

class _SiteIconState extends State<SiteIcon> {
  Uint8List? fetched;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(SiteIcon old) {
    super.didUpdateWidget(old);
    if (old.url != widget.url) {
      fetched = null;
      _load();
    }
  }

  void _load() {
    final url = widget.url;
    if (url == null || url.isEmpty) return;
    FaviconCache.instance.forUrl(url).then((bytes) {
      if (mounted && bytes != null && widget.url == url) {
        setState(() => fetched = bytes);
      }
    });
  }

  Widget get _globe =>
      Icon(Icons.public, size: widget.size, color: Colors.blueGrey);

  @override
  Widget build(BuildContext context) {
    Uint8List? bytes = fetched;
    if (bytes == null) {
      final b64 = widget.favicon;
      if (b64 != null && b64.isNotEmpty) {
        try {
          bytes = base64Decode(b64);
        } catch (_) {}
      }
    }
    if (bytes == null) return _globe;
    return ClipRRect(
      borderRadius: BorderRadius.circular(3),
      child: Image.memory(
        bytes,
        width: widget.size,
        height: widget.size,
        fit: BoxFit.contain,
        filterQuality: FilterQuality.medium,
        gaplessPlayback: true,
        errorBuilder: (_, _, _) => _globe,
      ),
    );
  }
}

/// File-type category for a downloaded file, grouped so related extensions
/// share one icon (archives together, audio together, …).
(IconData, Color) fileTypeLook(String name) {
  final ext = name.contains('.') ? name.split('.').last.toLowerCase() : '';
  const archives = {'zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'iso'};
  const audio = {'mp3', 'wav', 'flac', 'aac', 'm4a', 'ogg', 'wma'};
  const video = {'mp4', 'mkv', 'avi', 'mov', 'wmv', 'webm', 'flv', 'm4v'};
  const images = {
    'png',
    'jpg',
    'jpeg',
    'gif',
    'bmp',
    'webp',
    'svg',
    'heic',
    'ico',
  };
  const text = {'txt', 'md', 'log', 'csv', 'json', 'xml', 'yaml', 'yml'};
  const code = {
    'py',
    'js',
    'ts',
    'java',
    'c',
    'cpp',
    'h',
    'cs',
    'rs',
    'go',
    'rb',
    'php',
    'html',
    'css',
    'sql',
    'ipynb',
  };
  const installers = {'exe', 'msi', 'msix', 'appx'};
  if (ext == 'pdf') return (Icons.picture_as_pdf, _pdf);
  if (ext == 'doc' || ext == 'docx') return (Icons.description, _office);
  if (ext == 'xls' || ext == 'xlsx') return (Icons.grid_on, _excel);
  if (ext == 'ppt' || ext == 'pptx') return (Icons.slideshow, _ppt);
  if (archives.contains(ext)) {
    return (Icons.folder_zip, Colors.brown.shade600);
  }
  if (audio.contains(ext)) return (Icons.audiotrack, Colors.deepPurple);
  if (video.contains(ext)) return (Icons.movie, Colors.red.shade700);
  if (images.contains(ext)) return (Icons.image, Colors.teal);
  if (installers.contains(ext)) {
    return (Icons.install_desktop, Colors.indigo);
  }
  if (code.contains(ext)) return (Icons.code, _code);
  if (text.contains(ext)) return (Icons.article_outlined, Colors.blueGrey);
  return (Icons.insert_drive_file_outlined, Colors.blueGrey);
}

/// `2h 5m`, `12m`, `45s` — compact, for duration badges.
String formatDuration(int ms) {
  if (ms <= 0) return '0s';
  final totalSeconds = ms ~/ 1000;
  final h = totalSeconds ~/ 3600;
  final m = (totalSeconds % 3600) ~/ 60;
  final s = totalSeconds % 60;
  if (h > 0) return '${h}h ${m}m';
  if (m > 0) return '${m}m';
  return '${s}s';
}

String formatClock(int unixMs) {
  if (unixMs <= 0) return '';
  final t = DateTime.fromMillisecondsSinceEpoch(unixMs);
  String two(int v) => v.toString().padLeft(2, '0');
  final today = DateTime.now();
  final sameDay =
      t.year == today.year && t.month == today.month && t.day == today.day;
  return sameDay
      ? '${two(t.hour)}:${two(t.minute)}'
      : '${t.day}/${t.month} ${two(t.hour)}:${two(t.minute)}';
}
