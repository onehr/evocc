sqlite3 db :memory:
db eval {
  CREATE TABLE terrain(x INTEGER, y INTEGER, level INTEGER, PRIMARY KEY(x,y));
  WITH RECURSIVE x(n) AS (VALUES(0) UNION ALL SELECT n+1 FROM x WHERE n<63),
                 y(n) AS (VALUES(0) UNION ALL SELECT n+1 FROM y WHERE n<35)
  INSERT INTO terrain
  SELECT x.n,y.n, 600-(x.n-31)*(x.n-31)-3*(y.n-18)*(y.n-18)
      + ((x.n*17+y.n*31)%59) + 100*sin(x.n*0.3)*cos(y.n*0.3) FROM x,y;
  BEGIN; DELETE FROM terrain; ROLLBACK;
}
if {[db one {SELECT count(*) FROM terrain}] != 2304} {exit 1}
if {[db one {PRAGMA integrity_check}] ne "ok"} {exit 2}
puts [db one {
  SELECT json_object('width',64,'height',36,'cells',json_group_array(level),
                     'rows',(SELECT count(*) FROM terrain),'integrity','ok')
  FROM (SELECT level FROM terrain ORDER BY y,x)
}]
db close
exit 0
