/// Retain the teacher's cooperative Control choice across a transient session
/// interruption. Input itself is never replayed; the Student must acknowledge
/// a fresh Control request before the window can send keys or mouse events.
class ObserveControlRecovery {
  bool wanted = false;
  bool pending = false;
  String? surface, phase, error;
  bool connected = true;

  void choose(String mode) {
    wanted = mode != 'observe';
    pending = false;
  }

  bool update({
    required bool connected,
    required String? phase,
    required String? surface,
    required String mode,
    required String error,
    required bool ready,
    required bool requestPending,
  }) {
    const expired = 'Input lease expired; Student input released';
    final interrupted =
        (!connected && this.connected) ||
        ((phase == 'error' || phase == 'ended' || phase == 'starting') &&
            phase != this.phase) ||
        (this.surface != null && surface != null && surface != this.surface) ||
        (error == expired && error != this.error);
    if (wanted && interrupted) pending = true;
    this.connected = connected;
    this.phase = phase;
    this.surface = surface ?? this.surface;
    this.error = error;
    if (connected &&
        phase == 'live' &&
        mode != 'observe' &&
        error.isEmpty &&
        !requestPending) {
      wanted = true;
      pending = false;
    }
    if (!wanted ||
        !pending ||
        !connected ||
        phase != 'live' ||
        mode != 'observe' ||
        !ready ||
        requestPending ||
        (error.isNotEmpty && error != expired)) {
      return false;
    }
    pending = false; // One request per interruption, never a retry loop.
    return true;
  }
}
