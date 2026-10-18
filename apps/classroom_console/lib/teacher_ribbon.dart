import 'package:flutter/material.dart';
import 'icon_tints.dart';

/// Vietnamese UI strings for the Teacher main screen. Kept in one place so a
/// later language switch only has to replace this table.
class Vi {
  static const shareClass = 'Chiếu màn hình giảng viên';
  static const shareSelected = 'Chiếu cho máy đã chọn';
  static const presentStudent = 'Điều khiển & chiếu màn hình học viên';
  static const backToTeacher = 'Quay lại màn hình giảng viên';
  static const stopSharing = 'Dừng chiếu';
  static const presenting = 'Đang chiếu';
  static const pickMachines = 'Chọn máy';
  static const pickMachinesTip = 'Chọn nhiều máy để thao tác';
  static const donePicking = 'Xong chọn';
  static const selectAllOnline = 'Chọn tất cả máy online';
  static const clearSelection = 'Bỏ chọn';
  static const search = 'Tìm theo tên hoặc IP';
  static const rescan = 'Quét lại kết nối';
  static const scopeClass = 'Áp dụng cho: cả lớp';
  static const noStudents = 'Chưa có máy học viên';
  static const noMatch = 'Không có máy nào khớp';
  static const connecting = 'Đang kết nối lớp học…';
  static const waiting = 'Máy học viên sẽ tự xuất hiện tại đây khi kết nối.';
}

class RibbonItem {
  const RibbonItem({
    required this.icon,
    required this.label,
    required this.tooltip,
    this.onPressed,
    this.badge = 0,
    this.dot = false,
    this.active = false,
    this.badgeColor,
    this.menu,
    this.onMenu,
  });

  final IconData icon;
  final String label;
  final String tooltip;
  final VoidCallback? onPressed;

  /// Number shown in a badge (0 = hidden).
  final int badge;

  /// Small status dot instead of a number.
  final bool dot;

  /// Highlights a toggle that is currently on.
  final bool active;
  final Color? badgeColor;

  /// When set the item opens a menu instead of calling [onPressed].
  final List<PopupMenuEntry<String>> Function()? menu;
  final void Function(String value)? onMenu;
}

class RibbonTab {
  const RibbonTab(this.title, this.icon, this.items);
  final String title;
  final IconData icon;
  final List<RibbonItem> items;
}

const _teal = Color(0xff156b79);

/// One compact control strip. Row 1: [leading] | tabs ... [trailing]; row 2:
/// the labelled tools of the selected tab. Every tool has a visible label so
/// teachers do not have to hover unknown icons.
class TeacherRibbon extends StatelessWidget {
  const TeacherRibbon({
    super.key,
    required this.tabs,
    required this.index,
    required this.onTab,
    this.leading,
    this.trailing,
  });

  final List<RibbonTab> tabs;
  final int index;
  final ValueChanged<int> onTab;
  final Widget? leading;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final current = tabs[index.clamp(0, tabs.length - 1)];
    final tabStrip = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var i = 0; i < tabs.length; i++)
          _TabHeader(
            tab: tabs[i],
            selected: tabs[i] == current,
            onTap: () => onTab(i),
          ),
      ],
    );
    final divider = Container(
      width: 1,
      height: 24,
      margin: const EdgeInsets.symmetric(horizontal: 12),
      color: const Color(0xffdfe5eb),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        LayoutBuilder(
          builder: (context, c) {
            if (c.maxWidth >= 1150) {
              return Row(
                children: [
                  if (leading != null) ...[leading!, divider],
                  tabStrip,
                  Expanded(
                    child: Align(
                      alignment: Alignment.centerRight,
                      child: trailing,
                    ),
                  ),
                ],
              );
            }
            return Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: 12,
              runSpacing: 6,
              children: [
                ?leading,
                ConstrainedBox(
                  constraints: BoxConstraints(maxWidth: c.maxWidth),
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: tabStrip,
                  ),
                ),
                ?trailing,
              ],
            );
          },
        ),
        const SizedBox(height: 4),
        SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Row(
            children: [for (final item in current.items) _RibbonButton(item)],
          ),
        ),
      ],
    );
  }
}

class _TabHeader extends StatelessWidget {
  const _TabHeader({
    required this.tab,
    required this.selected,
    required this.onTap,
  });
  final RibbonTab tab;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => InkWell(
    onTap: onTap,
    borderRadius: BorderRadius.circular(6),
    child: Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
      decoration: BoxDecoration(
        border: Border(
          bottom: BorderSide(
            color: selected ? _teal : Colors.transparent,
            width: 2,
          ),
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            tab.icon,
            size: 16,
            color: selected
                ? IconTints.of(tab.icon)
                : IconTints.of(tab.icon).withValues(alpha: 0.6),
          ),
          const SizedBox(width: 6),
          Text(
            tab.title,
            style: TextStyle(
              fontSize: 12.5,
              fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
              color: selected ? _teal : Colors.blueGrey.shade700,
            ),
          ),
        ],
      ),
    ),
  );
}

class _RibbonButton extends StatelessWidget {
  const _RibbonButton(this.item);
  final RibbonItem item;

  @override
  Widget build(BuildContext context) {
    final enabled = item.menu != null
        ? item.onMenu != null
        : item.onPressed != null;
    final color = !enabled
        ? Colors.blueGrey.shade200
        : item.active
        ? _teal
        : const Color(0xff2b3a46);
    Widget icon = Icon(
      item.active && item.icon == Icons.visibility_outlined
          ? Icons.visibility
          : item.icon,
      size: 18,
      color: enabled ? IconTints.of(item.icon) : color,
    );
    if (item.badge > 0) {
      icon = Badge(
        label: Text('${item.badge}'),
        backgroundColor: item.badgeColor,
        child: icon,
      );
    } else if (item.dot) {
      icon = Badge(smallSize: 8, child: icon);
    }
    final face = Container(
      height: 34,
      padding: const EdgeInsets.symmetric(horizontal: 10),
      decoration: BoxDecoration(
        color: item.active ? const Color(0xffe1f0f2) : null,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          icon,
          const SizedBox(width: 7),
          Text(
            item.label,
            maxLines: 1,
            style: TextStyle(fontSize: 12, color: color),
          ),
        ],
      ),
    );
    return Tooltip(
      message: item.tooltip,
      waitDuration: const Duration(milliseconds: 500),
      child: item.menu != null
          ? PopupMenuButton<String>(
              enabled: enabled,
              tooltip: '',
              padding: EdgeInsets.zero,
              onSelected: item.onMenu,
              itemBuilder: (_) => item.menu!(),
              child: face,
            )
          : InkWell(
              borderRadius: BorderRadius.circular(8),
              onTap: item.onPressed,
              child: face,
            ),
    );
  }
}
