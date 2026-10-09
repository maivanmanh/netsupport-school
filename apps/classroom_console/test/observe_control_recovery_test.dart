import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/observe_control_recovery.dart';

void main() {
  bool update(
    ObserveControlRecovery r, {
    String phase = 'live',
    String surface = 'first',
    String mode = 'observe',
    String error = '',
    bool ready = true,
    bool pending = false,
    bool connected = true,
  }) => r.update(
    connected: connected,
    phase: phase,
    surface: surface,
    mode: mode,
    error: error,
    ready: ready,
    requestPending: pending,
  );

  test('Fresh Observe never becomes Control automatically', () {
    final r = ObserveControlRecovery();
    expect(update(r), isFalse);
    expect(update(r, surface: 'new'), isFalse);
  });
  test(
    'Confirmed Control is restored once after reopening and a fresh frame',
    () {
      final r = ObserveControlRecovery();
      update(r, mode: 'control');
      update(r, phase: 'error', error: 'Receiver stopped');
      expect(update(r, surface: 'new', ready: false), isFalse);
      expect(update(r, surface: 'new'), isTrue);
      expect(update(r, surface: 'new'), isFalse);
    },
  );
  test('Lease expiration reacquires Control without replaying held input', () {
    final r = ObserveControlRecovery();
    update(r, mode: 'control');
    const error = 'Input lease expired; Student input released';
    expect(update(r, error: error), isTrue);
    expect(update(r, error: error, pending: true), isFalse);
    expect(update(r, error: error), isFalse);
    update(r, mode: 'control');
    expect(update(r, error: error), isTrue);
  });
  test('Explicit Observe cancels retained Control intent', () {
    final r = ObserveControlRecovery();
    update(r, mode: 'control');
    r.choose('observe');
    update(r, mode: 'control', pending: true); // Old acknowledgement.
    update(r, phase: 'ended');
    expect(update(r, surface: 'new'), isFalse);
  });
  test('Permission errors and pending mode requests never trigger retries', () {
    final r = ObserveControlRecovery();
    update(r, mode: 'control');
    update(r, phase: 'error');
    expect(update(r, error: 'Remote input permission/init denied'), isFalse);
    expect(update(r, pending: true), isFalse);
    expect(update(r), isTrue);
    expect(update(r), isFalse);
  });
  test('Agent reconnect keeps intent; exclusive recovers cooperatively', () {
    final r = ObserveControlRecovery();
    update(r, mode: 'exclusive');
    update(r, connected: false);
    expect(update(r), isTrue);
    expect(update(r), isFalse);
  });
}
