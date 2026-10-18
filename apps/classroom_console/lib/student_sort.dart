/// How the Student roster is ordered. `none` keeps the order the agent reports.
enum StudentSort {
  none('none', 'Mặc định'),
  nameAsc('name_asc', 'Tên A → Z'),
  nameDesc('name_desc', 'Tên Z → A'),
  ipAsc('ip_asc', 'IP tăng dần'),
  ipDesc('ip_desc', 'IP giảm dần');

  const StudentSort(this.id, this.label);
  final String id;
  final String label;

  static StudentSort fromId(Object? id) =>
      values.firstWhere((s) => s.id == id, orElse: () => none);
}

/// Compares dotted IPv4 addresses numerically (`.9` before `.10`). Anything
/// that is not an IPv4 address sorts after the valid ones, by plain text.
int compareIps(String a, String b) {
  final x = _octets(a);
  final y = _octets(b);
  if (x == null || y == null) {
    if (x == null && y == null) return a.compareTo(b);
    return x == null ? 1 : -1;
  }
  for (var i = 0; i < 4; i++) {
    final c = x[i].compareTo(y[i]);
    if (c != 0) return c;
  }
  return 0;
}

List<int>? _octets(String ip) {
  final parts = ip.trim().split('.');
  if (parts.length != 4) return null;
  final values = parts.map(int.tryParse).toList();
  if (values.any((v) => v == null || v < 0 || v > 255)) return null;
  return values.cast<int>();
}

/// Natural, case-insensitive name comparison ("PC2" before "PC10").
int compareNames(String a, String b) {
  final x = _chunks(a.toLowerCase());
  final y = _chunks(b.toLowerCase());
  for (var i = 0; i < x.length && i < y.length; i++) {
    final p = x[i];
    final q = y[i];
    final pn = int.tryParse(p);
    final qn = int.tryParse(q);
    final c = pn != null && qn != null ? pn.compareTo(qn) : p.compareTo(q);
    if (c != 0) return c;
  }
  return x.length.compareTo(y.length);
}

List<String> _chunks(String s) =>
    RegExp(r'\d+|\D+').allMatches(s).map((m) => m[0]!).toList();

/// A sorted copy of [rows]; ties fall back to the other key so the order is
/// stable while Students come and go.
List<Map<String, dynamic>> sortStudents(
  List<Map<String, dynamic>> rows,
  StudentSort sort,
) {
  if (sort == StudentSort.none) return rows;
  String name(Map<String, dynamic> r) => '${r['name'] ?? ''}';
  String ip(Map<String, dynamic> r) => '${r['ip'] ?? ''}';
  final byName = sort == StudentSort.nameAsc || sort == StudentSort.nameDesc;
  final descending = sort == StudentSort.nameDesc || sort == StudentSort.ipDesc;
  int primary(Map<String, dynamic> a, Map<String, dynamic> b) {
    final c = byName
        ? compareNames(name(a), name(b))
        : compareIps(ip(a), ip(b));
    return descending ? -c : c;
  }

  return [...rows]..sort((a, b) {
    final c = primary(a, b);
    if (c != 0) return c;
    return byName ? compareIps(ip(a), ip(b)) : compareNames(name(a), name(b));
  });
}
