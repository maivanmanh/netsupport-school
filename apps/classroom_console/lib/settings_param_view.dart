import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'app_dialog.dart';
import 'icon_tints.dart';
import 'settings_params.dart';

/// The controls for every [Param] of one Settings page: the ordinary ones, then
/// the rarely needed ones under a collapsed "Nâng cao" heading. Each setting
/// shows its unit, a short explanation and a button that puts back the value
/// the app ships with; the page ends with a reset for the whole page.
class ParamPageView extends StatelessWidget {
  const ParamPageView({super.key, required this.store, required this.page});
  final ParamStore store;
  final int page;

  List<Param> get _params => [
    for (final p in store.params)
      if (p.page == page) p,
  ];

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: store,
      builder: (context, _) {
        final basic = _params.where((p) => !p.advanced).toList();
        final advanced = _params.where((p) => p.advanced).toList();
        final everything = _params;
        final anyChanged = everything.any((p) => !store.isDefault(p.id));
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            for (final p in basic) ParamField(store: store, param: p),
            if (advanced.isNotEmpty)
              Theme(
                data: Theme.of(
                  context,
                ).copyWith(dividerColor: Colors.transparent),
                child: ExpansionTile(
                  key: ValueKey('advanced-$page'),
                  tilePadding: EdgeInsets.zero,
                  childrenPadding: const EdgeInsets.only(top: 6),
                  initiallyExpanded: advanced.any((p) => store.isChanged(p.id)),
                  title: const Text(
                    'Nâng cao',
                    style: TextStyle(
                      fontWeight: FontWeight.w700,
                      fontSize: 13.5,
                    ),
                  ),
                  subtitle: const Text(
                    'Thường không cần đổi',
                    style: TextStyle(fontSize: 12, color: AppDialog.muted),
                  ),
                  children: [
                    for (final p in advanced)
                      ParamField(store: store, param: p),
                  ],
                ),
              ),
            if (everything.isNotEmpty)
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton.icon(
                  key: ValueKey('reset-page-$page'),
                  onPressed: anyChanged
                      ? () => store.resetAll(everything.map((p) => p.id))
                      : null,
                  icon: const TIcon(Icons.restart_alt, size: 18),
                  label: const Text('Đặt lại trang này về mặc định'),
                ),
              ),
          ],
        );
      },
    );
  }
}

/// One setting with its unit, help text and reset button.
class ParamField extends StatelessWidget {
  const ParamField({super.key, required this.store, required this.param});
  final ParamStore store;
  final Param param;

  @override
  Widget build(BuildContext context) {
    final p = param;
    final Widget control = switch (p.kind) {
      ParamKind.integer => TextField(
        key: ValueKey(p.id),
        controller: store.controllers[p.id],
        keyboardType: TextInputType.number,
        inputFormatters: [FilteringTextInputFormatter.digitsOnly],
        decoration: InputDecoration(
          labelText: p.label,
          suffixText: p.unit.isEmpty ? null : p.unit,
          helperText: p.help.isEmpty ? null : p.help,
          helperMaxLines: 3,
        ),
      ),
      ParamKind.text => TextField(
        key: ValueKey(p.id),
        controller: store.controllers[p.id],
        decoration: InputDecoration(
          labelText: p.label,
          hintText: p.hint,
          helperText: p.help.isEmpty ? null : p.help,
          helperMaxLines: 3,
        ),
      ),
      ParamKind.choice => DropdownButtonFormField<Object>(
        key: ValueKey('${p.id}-${store.value(p.id)}'),
        initialValue: store.value(p.id),
        isExpanded: true,
        decoration: InputDecoration(
          labelText: p.label,
          helperText: p.help.isEmpty ? null : p.help,
          helperMaxLines: 3,
        ),
        items: [
          for (final e in p.options!.entries)
            DropdownMenuItem(value: e.key, child: Text(e.value)),
        ],
        onChanged: (v) {
          if (v != null) store.set(p.id, v);
        },
      ),
      ParamKind.toggle => SwitchListTile(
        key: ValueKey(p.id),
        contentPadding: EdgeInsets.zero,
        title: Text(p.label),
        subtitle: p.help.isEmpty ? null : Text(p.help),
        value: store.value(p.id) as bool,
        onChanged: (v) => store.set(p.id, v),
      ),
    };
    return Padding(
      padding: const EdgeInsets.only(bottom: 14),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(child: control),
          SizedBox(
            width: 40,
            child: store.isDefault(p.id)
                ? null
                : Tooltip(
                    message: 'Đặt lại về mặc định (${_defaultText(p)})',
                    child: IconButton(
                      key: ValueKey('reset-${p.id}'),
                      visualDensity: VisualDensity.compact,
                      onPressed: () => store.reset(p.id),
                      icon: const TIcon(Icons.restart_alt, size: 18),
                    ),
                  ),
          ),
        ],
      ),
    );
  }

  static String _defaultText(Param p) {
    final d = p.def;
    if (p.kind == ParamKind.choice) return p.options![d] ?? '$d';
    if (d is bool) return d ? 'bật' : 'tắt';
    final text = '$d'.isEmpty ? 'để trống' : '$d';
    return p.unit.isEmpty ? text : '$text ${p.unit}';
  }
}
