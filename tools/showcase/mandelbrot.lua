print(_VERSION .. " / Mandelbrot in floating-point Lua")
local palette = " .:-=+*#%@"
for y = -6, 6 do
    local line = {}
    for x = 0, 49 do
        local cr, ci = -2.1 + x * 3.0 / 49, y * 0.16
        local zr, zi, n = 0, 0, 0
        while zr * zr + zi * zi <= 4 and n < 40 do
            zr, zi = zr * zr - zi * zi + cr, 2 * zr * zi + ci
            n = n + 1
        end
        local i = n == 40 and 10 or math.min(9, 1 + math.floor(n / 3))
        line[#line + 1] = palette:sub(i, i)
    end
    print(table.concat(line))
end
