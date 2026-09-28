group_by(.item) | map({item: .[0].item, units: (map(.units) | add), revenue: (map(.units * .price) | add)})
