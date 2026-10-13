import 'dart:convert';
import 'package:classroom_console/agent.dart';

/// In-memory stand-in for the Rust agent's settings database. Values are
/// JSON round-tripped like the real IPC so tests cannot rely on identity.
class FakeStore {
  final Map<String, Map<String, dynamic>> data = {};

  Map<String, dynamic> list(String namespace) => {
    for (final e in (data[namespace] ?? const <String, dynamic>{}).entries)
      e.key: jsonDecode(jsonEncode(e.value)),
  };

  void apply(
    String namespace,
    Map<String, dynamic> changes, {
    bool replace = false,
  }) {
    final docs = replace ? <String, dynamic>{} : (data[namespace] ??= {});
    for (final e in changes.entries) {
      if (e.value == null) {
        docs.remove(e.key);
      } else {
        docs[e.key] = jsonDecode(jsonEncode(e.value));
      }
    }
    data[namespace] = docs;
  }
}

/// Gives a test Agent a settings store without a socket.
mixin FakeStoreAgent on Agent {
  FakeStore get store;

  @override
  Future<Map<String, dynamic>> storeList(String namespace) async =>
      store.list(namespace);

  @override
  Future<void> storeApply(
    String namespace,
    Map<String, dynamic> changes, {
    bool replace = false,
  }) async => store.apply(namespace, changes, replace: replace);
}
