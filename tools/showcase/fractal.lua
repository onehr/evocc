local w, h, limit = 192, 108, 120
io.write(string.format('{"width":%d,"height":%d,"rgb":"',w,h))
for y=0,h-1 do
    for x=0,w-1 do
        local cr, ci = -2.15+x*3.05/(w-1), -0.86+y*1.72/(h-1)
        local zr, zi, n = 0,0,0
        while zr*zr+zi*zi <= 4 and n<limit do
            zr,zi=zr*zr-zi*zi+cr,2*zr*zi+ci
            n=n+1
        end
        local r,g,b=7,12,25
        if n<limit then
            local t=(n+1-math.log(math.log(math.sqrt(zr*zr+zi*zi)))/math.log(2))/limit
            r=math.floor(255*(0.5+0.5*math.cos(6.28318*(3*t+0.05))))
            g=math.floor(255*(0.5+0.5*math.cos(6.28318*(3*t+0.45))))
            b=math.floor(255*(0.5+0.5*math.cos(6.28318*(3*t+0.65))))
        end
        io.write(string.format('%02x%02x%02x',r,g,b))
    end
end
io.write('"}\n')
