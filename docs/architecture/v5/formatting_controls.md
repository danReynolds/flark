# Reading and setting inline formatting

Formatting state belongs to the shared `FlarkEditor`. Both host controllers
expose the same convenience methods, so an application toolbar does not need
to interpret Markdown or maintain its own toggled flags.

```dart
final bold = controller.styleState(Style.strong);
// bold.value: FlarkStyleValue.off / on / mixed
// bold.isOn, bold.isMixed, bold.canToggle
// bold.canEnable / bold.canDisable: eligibility for each explicit action

controller.setStyle(Style.strong, enabled: true);
controller.setStyle(Style.strong, enabled: false);

// Without a Flutter or Fleury controller:
final italic = editor.styleState(Style.emphasis);
editor.apply(const SetStyle(Style.emphasis, enabled: true));
```

Read again when the editor/controller notifies. Selection movement, keyboard
shortcuts, formatting, pending typing intent, Undo/Redo and mode changes all
use that same notification path. Queries do not parse Markdown. Use
`Style.strong`, `Style.emphasis`, `Style.strikethrough` or `Style.code`, one at
a time. Other style bits are not formatting commands.

At a collapsed caret, state describes the next typed character, including
formatting enabled before any text exists. Setting a style uses the existing
caret semantics: at the edge of a matching span it steps across the hidden
delimiter; inside a matching span, removing the style unwraps that span.
Changing selection clears pending intent as before.

For a range, state covers visible selected text, independently of selection
direction or which hidden-delimiter anchor was selected. Whitespace-only
pieces are ignored because Markdown emphasis cannot enclose only whitespace.
A mixed selection toggles **on**. Setting on/off normalizes fully selected
matching spans while preserving other styles, visible content and metadata.
The resulting Markdown is parsed and checked before publishing one undoable
change. Repeating an already-satisfied `SetStyle` returns `false` with
`lastRejection == null`, without changing revision, history or notifying.

Availability and selection are separate: bold may be active in a range for
which a removal is unsupported. The current bounded editing contract supports
a single inline row/table cell and complete matching style spans. Cross-block
ranges, partial matching spans, code fences and source mode disable those
controls. Inline code also prevents adding emphasis inside its literal text.
Even an eligible edit can be refused if Markdown cannot express it safely or
document admission limits would be exceeded; inspect `lastRejection` (or the
Flutter controller's notice). Availability is not an application permission:
custom controls must additionally apply the host's read-only/access policy.

The stock Flutter toolbar uses a filled background for active controls, an
outlined minus indicator for mixed selections, and disabled controls for
unavailable edits. Fleury uses inverse active buttons and a minus for mixed
selections. Both expose selected/mixed/disabled accessibility state and use
this API; neither maintains separate formatting state.
