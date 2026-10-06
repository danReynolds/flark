import 'package:web/web.dart' as web;

/// A browser drops a composition whose text the page replaces, without the
/// compositionend that tells Flutter's web engine it ended. Until another one
/// starts, the engine then reports the dropped composition's range as
/// composing with every change: the editor composed again and left editing
/// keys to the input element, or refused input whose range had become
/// invalid. Send the engine the event the browser did not.
void endDroppedComposition() {
  final element = web.document.activeElement;
  if (element == null || !element.classList.contains('flt-text-editing')) {
    return;
  }
  element.dispatchEvent(
    web.CompositionEvent('compositionend', web.CompositionEventInit(data: '')),
  );
}
