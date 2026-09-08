# Markdown theming and replaceable editor controls

Status: Flutter implementation and local qualification complete, 2026-09-08.
The single-editor playground has visual color selection, custom controls and
verified configuration export. The merge review fixes the observed semantic
text-field input failure and adds a real Chrome regression; full assistive-
technology and device qualification remain open. See the
[landing review](theming_merge_review_2026_09_08.md) and
[implementation history](theming_controls_review_2026_09_08.md).
The Fleury example remains T4 with M4 below. Existing native, performance and
release gates remain open; the owner's local-only CI exception remains in force.

## Intended result

An application can embed an editor and a matching viewer that fit its design
system without forking Flark. Ordinary color and typography changes require
theme values. Different controls require small presentation hooks. Neither
requires the application to reproduce Markdown recognition, selection rules,
stale-edit protection or undo behavior.

Keep the existing package boundary: shared Markdown semantics and commands in
`flark`; rendering, native styles, focus and overlays in each host. Implement
the Flutter path first and use the same semantic roles when `flark_fleury` is
built. A terminal cell style and a Flutter text style need different types.

The deliverables are runnable **theme playground example apps** in
`packages/flark_flutter/example` and `packages/flark_fleury/example`. Consumers
must be able to explore the options, change them interactively and take the
resulting configuration into their own application. The playgrounds are the
primary way to demonstrate and qualify this API, beginning with T1, rather
than documentation added after the implementation.

## Starting evidence (before this implementation)

- Flutter's [editor](../../../packages/flark_flutter/lib/src/editor.dart)
  exposes one base `TextStyle`. Its viewer does not forward a style option.
- The [surface](../../../packages/flark_flutter/lib/src/surface.dart) hardcodes
  link, code, selection and caret colors; quote/table/rule decorations;
  heading sizes; indentation; row spacing; and image slot geometry. Passing a
  different base text color cannot make this a complete dark theme.
- The [resource dialog](../../../packages/flark_flutter/lib/src/resource_dialog.dart)
  uses Material controls that already inherit parts of `ThemeData`, but its
  presentation cannot be replaced. The editor owns its invocation, target
  revision guard and focus restoration. Keep those responsibilities together.
- Ordinary link clicks currently place the caret. Cmd/Ctrl-click and the
  explicit open action route through `onOpenLink`; there is no contextual
  popover. External navigation remains unverified in Codex's embedded browser.
- Tree-sitter already emits theme-independent capture information. The Flutter
  surface maps token kinds to colors. Theming needs no new parser or second
  highlighter.
- The adjacent Fleury checkout has `ThemeData.extensions`, semantic color
  roles, `CellStyle`, focus/selection styles and overlays. Its extensions are
  plain typed objects; Flutter extensions implement `ThemeExtension`.
  `flark_fleury` itself is still unimplemented.
- V2 contains theme/popover code under `legacy/flark_flutter`. Use its covered
  roles and scenarios as audit input. Its controller/action dependencies do
  not belong in V5.

## Ownership and customization

| Concern | Owner | Consumer extension |
| --- | --- | --- |
| Markdown meaning, resource ranges, source edits, selection and history | `flark` and its parser | Existing semantic commands |
| Snippet analysis and token classification | `flark_tree_sitter` and the existing code adapter | Theme-independent analysis API |
| Document typography, colors, spacing and painted decorations | `flark_flutter`; later `flark_fleury` | Host-specific theme data |
| Popover placement, editor focus, request lifetime and guarded actions | Respective host adapter | Replace the displayed controls; retain guarded actions |
| Buttons, fields, menus and dialogs | Host toolkit defaults | Ambient toolkit theme; custom builder/presenter when needed |
| URL opening, relative URI resolution and image fetching | Application through existing host hooks | `onOpenLink`, `baseUri`, `imageProvider` |

Do not introduce a universal widget, color, font, geometry or overlay abstraction
into the kernel. Share the existing semantic facts and a documented vocabulary
of styling roles. A product used on both hosts can map its design tokens into
the two native themes. Extract additional portable data only when both hosts
demonstrate the same need.

## Document appearance

Add an immutable `FlarkThemeData` in `flark_flutter`, registered through
Flutter's `ThemeData.extensions`, with an optional per-widget `theme` override
on both editor and viewer. These are two scopes of the same data type.
Implement `copyWith`, merging and Flutter's `lerp` contract. Group fields only
where there is a coherent value, such as a code palette or block decoration;
avoid both a giant flat bag and one class per Markdown token.

Resolve one complete theme at the host boundary, in this order:

1. Defaults derived from the surrounding toolkit theme and text style.
2. The application's ambient Flark theme extension.
3. A specific editor/viewer's Flark theme overrides.

The existing editor `style` becomes a compatibility override for body
typography at the final layer. Make its default absent so its old hardcoded
color cannot accidentally override dark mode. Explicit role styles continue
to override the resulting body style. Give the viewer the same forwarding
behavior. Document this precedence with a runnable example.

| Role group | Customizable values and default direction |
| --- | --- |
| Document | Body text, canvas color, document insets, paragraph spacing; inherit app typography and a coherent surface/foreground pair |
| Headings | H1–H6 styles and spacing; derive a readable hierarchy from body typography |
| Inline formatting | Strong, emphasis, strike, link and inline-code styles; links inherit the app primary color and remain underlined |
| Lists and quotes | Marker style, indentation, task states, quote text/rail/background; use app foreground, primary and divider roles |
| Tables and rules | Cell padding, header style/fill, grid and rule appearance |
| Fenced code | Monospace base style, background, padding/radius and semantic syntax palette; choose a light/dark palette from host brightness |
| Images | Alt/caption and placeholder style, frame decoration and bounded preview-slot dimensions |
| Editing feedback | Caret, selection and composition appearance where supported; derive from host editing conventions |

Only expose roles for supported Markdown presentation. A theme cannot change
which source characters are hidden, replace a text run with an arbitrary
widget, or change document semantics. Preserve combined styles: for example,
a bold, struck link must retain bold, strike and its link cue together.
Changing a code palette recolors existing classifications; it must not restart
analysis. Token overrides are foreground colors only in this slice: asynchronous
highlighting must not change font weight, font family or glyph geometry. The
code block's base typography remains customizable through the synchronous
host theme/layout update. Unknown token kinds retain the base code foreground.

The default document surface should be explicit enough to pair colors
correctly. A transparent document surface is an explicit consumer override.
Leave maximum editor width and outer page layout to the embedding application.
Keep image decode/cache limits as resource policy rather than theme settings.

### Link colors

Use the host's primary/accent role, plus a persistent underline. Applications
can override the link style independently of button colors. There is no
universal fixed blue and no automatic visited-link state in the initial API.
Test built-in light/dark palettes for normal text contrast of at least 4.5:1
against their actual backgrounds, including the code and popover surfaces.
Document that custom brand colors need the same checks; do not silently
substitute a different brand color at runtime. Underlining supplies a cue that
survives loss of color, including Fleury's `NO_COLOR` path.

## Controls and link behavior

Use existing toolkit component themes for routine control styling. Flark's
document theme should not duplicate the toolkit's button, text-field, menu or
dialog theme. Include only Flark-specific presentation values that a real
default control needs, rather than guessing every future control property.

Provide two narrow extension points, with names provisional until the first
consumer example compiles:

- `linkPopoverBuilder(context, actions)`: replace the popover contents and
  visual container. The host retains anchoring, visibility and dismissal.
  The supplied immutable view data includes the label/destination and guarded
  Open, Edit, Remove and Dismiss actions with availability state.
- `presentResourceEditor(context, session)`: replace link/image editing with
  an application dialog, sheet or panel. A bounded session exposes the
  original values and guarded apply/remove actions that report acceptance or
  rejection. The default Material editor uses the same contract. Presentation
  code closes its own route after success or cancel; Flark invalidates the
  session and manages editor focus on completion.

The names above describe capabilities, not an extensible command registry.
Consumers do not receive a requirement to build Markdown strings or restore
source ranges themselves. Typed drafts are converted into existing kernel
commands by the host. Keep request/session details host-owned initially;
extract shared nonvisual pieces when the second host exercises them.

Bind every session/action to its controller, document revision and captured
selection/target. A later source edit, selection change, source-mode switch,
read-only transition, controller replacement or disposal makes an obsolete
action inert. Do not guess how to rebase an old popover target. The same guard
must apply to default and custom controls. Restore editor focus only if the
same editor still owns the interaction; never reclaim it from a newly focused
control or editor. Preserve the original error-in-dialog experience for
rejected edits, and make cancellation mutation-free.

Default interaction:

- A completed ordinary click/tap in a link places the caret and shows the
  small destination/Open/Edit/Remove popover. It does not steal text focus.
- A text-selection drag does not open the popover. Typing, Escape, moving
  outside the target or clicking elsewhere dismisses it without consuming
  the edit. Shift-F10/the context-menu key offers keyboard access to these
  actions at the caret without repurposing Tab from indentation/table editing;
  Cmd/Ctrl-K continues to edit the link directly. Include the actions in the
  accessibility semantics rather than relying on shortcut knowledge alone.
- Cmd/Ctrl-click opens through the existing callback. Read-only links retain
  ordinary-click opening. Disabled actions are truthful when no open handler
  or valid destination exists; do not force a URL launcher dependency on clients.
- Wrapped links anchor near the activated fragment. Scroll, resize, reflow
  and text scaling reposition the popover or close it if the target leaves
  view. Placement respects the viewport and any on-screen keyboard.
- The popover remains nonmodal while typing. Keyboard navigation can enter
  its actions and return to the original caret; action labels and focus cues
  remain usable without hover. A touch scroll must not activate it.

Continue supporting `showToolbar: false` plus public controller commands for
applications with their own toolbars. Do not add a generic toolbar/plugin
framework in this slice.

## Fleury adoption

`flark_fleury` should offer the equivalent semantic roles using `CellStyle`,
cell spacing/borders and Fleury's typed theme extensions. It should derive
foreground/background, primary, focus and selection from the ambient Fleury
theme and preserve terminal-default colors where appropriate. There are no
pixel font sizes, shadows or radii to emulate on a terminal.

Use Fleury widgets and overlays for link controls, consuming the same kernel
commands. Test action availability and semantic results across both hosts;
do not require pixel-identical presentation or identical platform gestures.
Terminal hyperlink capability, modifier handling and URL opening need their
own host qualification. Browser Fleury may render genuine anchors where its
capabilities support them. An explicit keyboard Open action must remain
available when a terminal cannot deliver modified clicks.

Do not route Flark documents through Fleury's existing lightweight
`MarkdownText` recognizer or add another code highlighter. Reuse its toolkit
components and theme conventions, with Flark's existing parsed model.

## Package example apps

Extend the existing Flutter example with a clearly discoverable Theme
playground, retaining its draft and qualification workflows. Build the Fleury
example in its own package as part of the host milestone. Both apps expose the
same semantic categories through controls that suit their toolkit:

1. **Appearance:** host defaults, light/dark and a distinct custom preset;
   document foreground/background, accent and independent link style.
2. **Typography:** body and heading styles, inline formatting and code text;
   Flutter font/size/line-height controls and Fleury's supported cell attributes.
3. **Blocks:** lists, tasks, quotes, tables, rules, code frames and image
   presentation, with the spacing and decoration options supported by the host.
4. **Syntax colors:** edit the semantic token palette and see real snippets
   recolor using existing Tree-sitter analysis.
5. **Controls:** switch between default and branded link popovers, and default
   and alternate resource-editing presentations. Actually open, edit and remove
   links/images through these controls; do not substitute static mockups.

Every category has an immediately visible Markdown example and a short
explanation of what the setting changes. Following the owner's 2026-09-08
simplification, show one live editor beside the theming panel (below it on
narrow windows). Color controls include a visual picker, opacity and hex entry.
Keep viewer/theme parity in host tests rather than duplicating the example's
document and controller. Keep a useful mixed document with
headings, combined inline styles, links, image success/failure, lists/tasks,
quotes, tables and representative code snippets.

Changing a setting updates the editor live while retaining document edits,
selection and history. Include Reset theme separately from Reset sample so
experimenting with appearance never silently erases writing or saved drafts.
Demonstrate both ambient application theming and a per-instance override.

Provide a **Copy Dart configuration** action and a readable code view. Export
the current overrides, relevant imports and editor wiring using public
APIs; inherited values should remain inherited. A control-presentation choice
includes or links directly to the complete runnable builder/presenter example
so the exported configuration does not reference an unexplained private helper.
On terminals without clipboard support, retain the selectable/readable code
view and report copy availability honestly.

Keep these as ordinary consuming applications: no imports from package `src/`,
no direct parser access for UI operations, and no theme or editing implementation
hidden in the showcase that a normal client would have to duplicate. There is
no extra theme-preset file format or preset-sharing service in this slice.
Each README starts with how to run the example and a short customization tour,
then links to qualification-only workflows separately.

The example acceptance journey is: start from host defaults, change link color
and a block style, switch light/dark, choose custom controls, edit a link,
continue typing, undo, copy the configuration and reproduce it in a minimal
consumer. Exercise font/spacing controls on Flutter and constrained-color and
no-color presentation on Fleury. Verify that exported configurations compile
and reproduce the chosen settings; screenshots alone do not prove the example
is usable. A Flutter-only delivery closes its host slice, not the promised
two-host example deliverable.

## Milestones

| Milestone | Work | Exit and reflection checkpoint |
| --- | --- | --- |
| T1 — Prove link customization | Establish theme resolution for body/surface/link and editing feedback; add the contextual popover and guarded presentation hooks; create the first interactive link/control page in `flark_flutter/example` with editor and viewer | The playground changes link color and swaps default/branded popovers and editing controls through public APIs. Run click/edit/remove/cancel/undo/next-character journeys in both presentations. Review the public API before widening it. |
| T2 — Complete Markdown theme coverage | Move remaining styles, decorations, spacing and code palette into the same resolved theme; expand the playground categories and handle theme/metric invalidation | Consumers can customize supported Markdown roles live in both editor and viewer. No feature-specific palette remains buried in painting code. Typography changes keep caret/hit-testing current on the first frame. Review field count, ownership and default contrast. |
| T3 — Deliver the Flutter example | Complete presets, ambient/per-instance examples, reset behavior, Dart configuration export and the README tour; qualify the example through public APIs | Real browser dogfooding passes the full customization/editing journey. Copied configurations compile and reproduce settings in a minimal consumer. Record local evidence and limitations. Review whether the two hooks are sufficient before freezing them. |
| T4 — Deliver the Fleury example with M4 | Implement native Fleury theme mapping and controls, plus the equivalent interactive playground in `flark_fleury/example` | Same semantic customization/editing journeys pass with Fleury input, terminal/no-color/cell presentation and browser behavior. Its README and exported configurations work through public APIs. Adjust shared vocabulary only for a demonstrated second-host need. |

T1 and T2 are implemented with local automated evidence. T3's runnable example,
configuration export and consumer checks are implemented; its real-browser
acceptance journey remains open. The review records the API and invalidation
decisions, regressions found, and precise evidence boundaries. T4 belongs to
the existing M4 workstream rather than blocking the Flutter improvement.

## Qualification and performance constraints

Test independently specified product outcomes, not only theme object merging
or whether the builder was invoked:

1. On the first relevant paint, verify text/style combinations, selection,
   caret and control placement for links inside headings, emphasis, strike,
   tables and wrapped paragraphs. Include image states and code selection.
2. Exercise default and custom controls through real input: click, drag,
   keyboard action focus, touch-scroll cancellation, edit, remove, cancel,
   undo/redo and the next typed character. Check source and history as well
   as visible outcome. Late actions after controller/selection/source changes
   must reject without mutation or focus theft.
3. Switch light/dark/custom themes while a caret, selection, popover, loaded
   image and pending code analysis exist. Colors may change; source,
   selection and undo history do not. Old async work must use the current
   theme and never reinstate stale colors.
4. Change body/code fonts, size, line height, text scaling, spacing and width;
   validate reflow, caret reveal, IME geometry, hit targets and popover anchor
   on the first frame. A font change is a geometry change, not a repaint-only
   update. Check narrow windows and a large-text example.
5. Use palette snapshots/paint assertions for representative appearance and
   actual browser dogfooding for interaction quality. Test built-in contrast
   and non-color cues without claiming this slice is a full accessibility
   certification. The Fleury lane adds constrained-color and `NO_COLOR` cases.

Theme resolution must not run Markdown parsing, change parser admission or
request new Tree-sitter work. Preserve existing paragraph reuse when the
resolved theme is unchanged. A color update can require refreshed text spans;
it must preserve geometry. A metric change must invalidate the affected layout
and both live/source-mode caches before the next paint. Do not promise a
paint-only Flutter operation for every text color change.

The current `_clearRows()` also clears image streams. Separate paragraph
invalidation from image-resource invalidation where theme updates require it,
so changing link color does not refetch images. Preserve fixed preview-slot
geometry across loading/success/failure, even with custom appearance. Keep
layout-affecting values finite and valid, and preserve bounded nesting/layout
behavior under large consumer overrides.

Run local analysis and the relevant mounted resource/layout/code tests as each
slice lands, then the host/workbench and browser suites for the completed
candidate. Kernel/native-parser tests are needed if their code changes; a
theme-only edit does not require rebuilding parser assets. Recheck interaction
costs if new layout invalidation or per-keystroke overlay work changes them.
Existing production-line budgets still apply. This plan is not authorization
to claim closed native performance gates or to commit/push the implementation.

## Sources and design checks

Flutter's [ThemeExtension contract](https://api.flutter.dev/flutter/material/ThemeExtension-class.html)
supports typed theme additions and interpolation; use its existing mechanism.
W3C's [Use of Color guidance](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color)
supports retaining a non-color link cue, and its
[text contrast technique](https://www.w3.org/WAI/WCAG22/Techniques/general/G18)
describes checking foregrounds against their actual backgrounds.

Fleury conventions were checked directly in the adjacent checkout on
2026-09-08: `packages/fleury/lib/src/widgets/theme.dart`,
`packages/fleury/lib/src/widgets/overlay.dart`, and
`packages/fleury_widgets/lib/src/component_theme.dart`. These establish a
compatible host direction; they are not evidence of a completed Flark adapter.
