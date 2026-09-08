# Links and image previews

Implement after the reviewed formatting/layout checkpoint and native
qualification attempt. Keep the foreground macOS gate explicit if the host
cannot establish the required lifecycle.

1. Expose Comrak's resolved destination and title in the render model for
   links, autolinks and images. This includes reference definitions, entities
   and escaped destinations; the host must not recognize Markdown again.
2. Add kernel commands to insert/edit links and images and remove them.
   Preserve selected inline formatting when only the destination changes.
   Validate the candidate's intended resource through the parser before
   publishing one undoable source/caret/history transaction. Reject code,
   cross-block and stale edits without mutation.
3. Add Link (Cmd/Ctrl-K) and Image toolbar controls with small editing dialogs.
   Preserve focus and selection through the dialog, reject stale submissions,
   and provide an explicit open-link action through a host callback.
4. Render bounded image previews below editable alt text in the existing
   surface. Reserve geometry before loading; loading, failure and success must
   not move the caret or following text. Resolve only visible images, bound the
   retained cache and decode size, and dispose listeners/images on replacement.
   Let consumers supply image providers and a base URI for relative resources.
5. Verify parser/transport identity, source/caret/history/next-character
   journeys, mounted dialogs and first-paint geometry, then actual browser
   authoring, opening, preview loading/failure and Undo/Redo. Rebuild the normal
   web and native workbench. CI remains skipped at the owner's request.

Attachment upload/storage, embeds, image resizing and rich link cards are
outside this slice. Markdown URLs and alt text remain portable document data.
