import 'package:fleury/fleury_core.dart';

// Text accents for the example's page and code backgrounds. Each preset has
// a light and dark counterpart; arbitrary hex choices remain caller-owned.
const accentPresets = <({String name, RgbColor dark, RgbColor light})>[
  (name: 'Slate', dark: RgbColor(203, 213, 225), light: RgbColor(51, 65, 85)),
  (name: 'Red', dark: RgbColor(252, 165, 165), light: RgbColor(185, 28, 28)),
  (name: 'Green', dark: RgbColor(134, 239, 172), light: RgbColor(22, 101, 52)),
  (name: 'Yellow', dark: RgbColor(253, 224, 71), light: RgbColor(146, 64, 14)),
  (name: 'Blue', dark: RgbColor(147, 197, 253), light: RgbColor(29, 78, 216)),
  (
    name: 'Purple',
    dark: RgbColor(216, 180, 254),
    light: RgbColor(128, 34, 170),
  ),
  (name: 'Cyan', dark: RgbColor(103, 232, 249), light: RgbColor(0, 95, 130)),
  (name: 'Pink', dark: RgbColor(249, 168, 212), light: RgbColor(190, 24, 93)),
  (name: 'Gray', dark: RgbColor(148, 163, 184), light: RgbColor(71, 85, 105)),
  (name: 'Orange', dark: RgbColor(253, 186, 116), light: RgbColor(154, 52, 18)),
  (name: 'Teal', dark: RgbColor(94, 234, 212), light: RgbColor(15, 118, 110)),
  (name: 'Lime', dark: RgbColor(190, 242, 100), light: RgbColor(63, 98, 18)),
  (name: 'Indigo', dark: RgbColor(165, 180, 252), light: RgbColor(67, 56, 202)),
  (
    name: 'Violet',
    dark: RgbColor(196, 181, 253),
    light: RgbColor(109, 40, 217),
  ),
  (name: 'Rose', dark: RgbColor(253, 164, 175), light: RgbColor(190, 18, 60)),
  (name: 'Neutral', dark: RgbColor(255, 255, 255), light: RgbColor(25, 31, 37)),
];
