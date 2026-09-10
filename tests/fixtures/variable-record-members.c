void direct_member(int n) { struct record { int data[n]; }; }

void typedef_member(int n) { typedef int Row[n]; struct record { Row data; }; }

void union_member(int n) { typedef int Row[n]; union record { Row data; long other; }; }

void nested_array_member(int n) { typedef int Row[n]; struct record { Row data[2]; }; }
