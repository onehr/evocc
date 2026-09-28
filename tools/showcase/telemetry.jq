# Fictional rover sensor data, generated and aggregated by the actual jq binary.
[range(0;48) as $t | {
  t:$t, solar:([0, (((($t-8)*0.11)|sin)*90)]|max),
  battery:(78-($t*0.6)+(($t*0.18|sin)*12)),
  temp:(-25+(($t*0.13|sin)*35)), distance:($t*$t*0.018)
}] | {mission:"Aster rover", simulated:true, samples:.,
      peakSolar:(map(.solar)|max), distance:.[-1].distance}
