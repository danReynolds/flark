/// The D0 frame gate, shared by both profile harnesses and the receipt
/// validator.
///
/// An edit passes when the work on its critical path fits the budget and it
/// reaches the first frame that could show it:
///
/// - UI work: the command plus the frame's build (UI thread).
/// - Raster: the frame's raster duration (raster thread).
/// - Late frames: vsyncs that passed between the command finishing and the
///   frame that showed it. Zero means the next frame.
///
/// Input-to-raster latency is reported but not gated. Most of it is waiting
/// for vsync, which depends on where the input fell in the refresh cycle and
/// on the display's rate rather than on Flark's work.
library;

/// One 60 Hz frame. Budgets hold on 60 and 120 Hz displays alike.
const frameBudgetUs = 16667;

/// A vsync this close to a frame interval still counts as the next frame.
const vsyncJitterUs = 1000;

/// Nearest rank, so small groups require every sample to pass.
int nearestRankP99(List<int> values) {
  final sorted = [...values]..sort();
  return sorted[(sorted.length * .99).ceil() - 1];
}

/// Command time of a linked sample: `commandUs` (frame profile) or
/// `actionUs` (workbench profile).
int commandUs(Map sample) => (sample['commandUs'] ?? sample['actionUs']) as int;

int uiWorkUs(Map sample) => commandUs(sample) + (sample['buildUs'] as int);

int rasterUs(Map sample) => sample['rasterUs'] as int;

/// Frames skipped between the command finishing and the frame showing it.
int lateFrames(Map sample, num displayHz) {
  final interval = 1e6 / displayHz;
  final wait =
      (sample['vsyncStartUs'] as int) -
      ((sample['startUs'] as int) + commandUs(sample));
  if (wait <= interval + vsyncJitterUs) return 0;
  return ((wait - vsyncJitterUs) / interval).floor();
}

/// Gate results for one group of linked samples.
final class FrameGateResult {
  FrameGateResult(List<Map> samples, num displayHz, {required this.budgetUs})
    : samples = samples.length,
      uiP99Us = nearestRankP99([for (final s in samples) uiWorkUs(s)]),
      rasterP99Us = nearestRankP99([for (final s in samples) rasterUs(s)]),
      lateP99 = nearestRankP99([
        for (final s in samples) lateFrames(s, displayHz),
      ]),
      latencyP99Us = nearestRankP99([
        for (final s in samples) s['latencyUs'] as int,
      ]);

  final int samples, budgetUs, uiP99Us, rasterP99Us, lateP99, latencyP99Us;

  /// Failures of this group; empty when it passes. [nextFrame] is required
  /// for live edits and reflows, not for opening or source-mode work.
  List<String> failures(String group, {required bool nextFrame}) => [
    if (uiP99Us >= budgetUs) '$group: UI work $uiP99Us >= $budgetUs us',
    if (rasterP99Us >= budgetUs) '$group: raster $rasterP99Us >= $budgetUs us',
    if (nextFrame && lateP99 > 0) '$group: missed its next frame',
  ];

  Map<String, Object> toJson() => {
    'samples': samples,
    'budgetUs': budgetUs,
    'uiP99Us': uiP99Us,
    'rasterP99Us': rasterP99Us,
    'lateFramesP99': lateP99,
    'latencyP99Us': latencyP99Us,
  };
}
