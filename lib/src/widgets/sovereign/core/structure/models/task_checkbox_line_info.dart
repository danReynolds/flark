class TaskCheckboxLineInfo {
  const TaskCheckboxLineInfo({
    required this.markerStart,
    required this.taskStart,
    required this.contentStart,
    required this.isOrdered,
  });

  final int markerStart;
  final int taskStart;
  final int contentStart;
  final bool isOrdered;
}
