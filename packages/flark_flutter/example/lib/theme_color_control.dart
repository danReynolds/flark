import 'package:flex_color_picker/flex_color_picker.dart';
import 'package:flutter/material.dart';

/// An example-only color control. The picker and hex field share one value.
class ThemeColorControl extends StatefulWidget {
  const ThemeColorControl({
    super.key,
    required this.title,
    required this.color,
    required this.onChanged,
  });
  final String title;
  final Color color;
  final ValueChanged<Color> onChanged;

  @override
  State<ThemeColorControl> createState() => _ThemeColorControlState();
}

class _ThemeColorControlState extends State<ThemeColorControl> {
  late final code = TextEditingController(text: hex(widget.color));
  bool expanded = false;

  String hex(Color color) {
    final value = color.toARGB32().toRadixString(16).padLeft(8, '0');
    return (value.startsWith('ff') ? value.substring(2) : value).toUpperCase();
  }

  Color? parse(String value) {
    final text = value.replaceFirst('#', '');
    if (!RegExp(r'^(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$').hasMatch(text)) {
      return null;
    }
    return Color(int.parse(text.length == 6 ? 'ff$text' : text, radix: 16));
  }

  @override
  void didUpdateWidget(ThemeColorControl oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.color != oldWidget.color && parse(code.text) != widget.color) {
      code.text = hex(widget.color);
    }
  }

  @override
  void dispose() {
    code.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 10),
    child: Column(
      children: [
        Row(
          children: [
            IconButton(
              tooltip: '${expanded ? 'Close' : 'Choose'} ${widget.title}',
              onPressed: () => setState(() => expanded = !expanded),
              icon: ColorIndicator(
                color: widget.color,
                width: 32,
                height: 32,
                borderRadius: 6,
                hasBorder: true,
                borderColor: Theme.of(context).colorScheme.outline,
              ),
            ),
            const SizedBox(width: 6),
            Expanded(
              child: TextField(
                controller: code,
                decoration: InputDecoration(
                  labelText: widget.title,
                  prefixText: '#',
                  helperText: 'Hex · RRGGBB or AARRGGBB',
                ),
                onChanged: (value) {
                  final color = parse(value);
                  if (color != null) widget.onChanged(color);
                },
              ),
            ),
          ],
        ),
        if (expanded)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: ColorPicker(
              color: widget.color,
              onColorChanged: widget.onChanged,
              padding: EdgeInsets.zero,
              wheelDiameter: 200,
              enableOpacity: true,
              enableShadesSelection: false,
              pickersEnabled: const {
                ColorPickerType.primary: false,
                ColorPickerType.accent: false,
                ColorPickerType.wheel: true,
              },
            ),
          ),
      ],
    ),
  );
}
