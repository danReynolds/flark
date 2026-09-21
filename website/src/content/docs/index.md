---
title: Flark
description: Live Markdown editing for Flutter and Fleury.
---

Flark is a live Markdown composer for Flutter and Fleury. Your app keeps
Markdown; Flark handles rendering, selection, formatting, and undo history.

**[Read the approved Using Flark API design →](/guides/using-flark/)**

The guide is a short, approved design for the next API implementation.
It covers automatic loading, widget/controller ownership, editing methods,
and observable state. Its proposed APIs are not available yet.

| Package | Role |
| --- | --- |
| `flark` | Shared editing engine |
| `flark_flutter` | Flutter widgets, including web |
| `flark_fleury` | Fleury widgets for terminals and browsers |
| `flark_tree_sitter` | Optional code highlighting and indentation |

The current V5 development packages are not yet published to pub.dev.
