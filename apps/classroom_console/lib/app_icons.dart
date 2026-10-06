import 'dart:convert';
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
  const AppBadge({super.key, required this.app, this.size = 28});
  final String app;
  final double size;
  @override
  Widget build(BuildContext context) {
    final look = lookAppUp(app);
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

/// A site icon for a browser tab: the real favicon when we have one
/// (base64 PNG), otherwise a neutral globe placeholder.
class SiteIcon extends StatelessWidget {
  const SiteIcon({super.key, this.favicon, this.size = 18});
  final String? favicon;
  final double size;
  @override
  Widget build(BuildContext context) {
    final b64 = favicon;
    if (b64 == null || b64.isEmpty) {
      return Icon(Icons.public, size: size, color: Colors.blueGrey);
    }
    try {
      final bytes = base64Decode(b64);
      return ClipRRect(
        borderRadius: BorderRadius.circular(3),
        child: Image.memory(
          bytes,
          width: size,
          height: size,
          fit: BoxFit.cover,
          gaplessPlayback: true,
          errorBuilder: (_, _, _) =>
              Icon(Icons.public, size: size, color: Colors.blueGrey),
        ),
      );
    } catch (_) {
      return Icon(Icons.public, size: size, color: Colors.blueGrey);
    }
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
      : '${t.month}/${t.day} ${two(t.hour)}:${two(t.minute)}';
}
