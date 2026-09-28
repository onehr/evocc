sqlite3 db :memory:
puts "SQLite [db one {SELECT sqlite_version()}]"
puts "In-memory SQL / transactions / integrity"
db eval {
    CREATE TABLE sales(item TEXT, units INTEGER, price REAL);
    INSERT INTO sales VALUES ('coffee', 3, 4.50), ('tea', 5, 2.00), ('coffee', 2, 4.50);
}
puts ""
puts [format "%-12s %5s %9s" item units revenue]
db eval {SELECT item, sum(units) AS n, sum(units * price) AS total FROM sales GROUP BY item} {
    puts [format "%-12s %5d %9.2f" $item $n $total]
}
db eval {BEGIN; DELETE FROM sales; ROLLBACK;}
set rows [db one {SELECT count(*) FROM sales}]
set integrity [db one {PRAGMA integrity_check}]
puts ""
puts "After rollback: $rows rows"
puts "Integrity check: $integrity"
if {$rows != 3 || $integrity ne "ok"} {exit 1}
db close
exit 0
