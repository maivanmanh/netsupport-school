import 'package:classroom_console/student_sort.dart';
import 'package:flutter_test/flutter_test.dart';

Map<String, dynamic> row(String name, String ip) => {'name': name, 'ip': ip};

List<String> names(List<Map<String, dynamic>> rows) => [
  for (final r in rows) r['name'] as String,
];

void main() {
  final rows = [
    row('PC10', '10.0.55.10'),
    row('pc2', '10.0.55.9'),
    row('Alpha', '10.0.55.100'),
    row('PC1', 'not-an-ip'),
  ];

  test('IPs compare numerically, not as text', () {
    expect(compareIps('10.0.55.9', '10.0.55.10'), lessThan(0));
    expect(compareIps('10.0.55.100', '10.0.55.20'), greaterThan(0));
    expect(compareIps('10.0.55.1', '10.0.55.1'), 0);
    expect(compareIps('bad', '10.0.0.1'), greaterThan(0));
  });

  test('names are natural and case-insensitive', () {
    expect(names(sortStudents(rows, StudentSort.nameAsc)), [
      'Alpha',
      'PC1',
      'pc2',
      'PC10',
    ]);
    expect(names(sortStudents(rows, StudentSort.nameDesc)), [
      'PC10',
      'pc2',
      'PC1',
      'Alpha',
    ]);
  });

  test('IP order is numeric; invalid addresses go last', () {
    expect(names(sortStudents(rows, StudentSort.ipAsc)), [
      'pc2',
      'PC10',
      'Alpha',
      'PC1',
    ]);
    expect(names(sortStudents(rows, StudentSort.ipDesc)), [
      'PC1',
      'Alpha',
      'PC10',
      'pc2',
    ]);
  });

  test('default keeps the reported order and the input is not mutated', () {
    expect(names(sortStudents(rows, StudentSort.none)), names(rows));
    sortStudents(rows, StudentSort.nameAsc);
    expect(names(rows).first, 'PC10');
    expect(StudentSort.fromId('ip_desc'), StudentSort.ipDesc);
    expect(StudentSort.fromId('bogus'), StudentSort.none);
  });
}
