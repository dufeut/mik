-- Mixed workload simulating realistic usage
local counter = 0
local modules = {"auth", "api", "users", "data"}

request = function()
    counter = counter + 1
    local module = modules[(counter % #modules) + 1]
    local path = "/run/" .. module .. "/api/v1/resource"

    if counter % 5 == 0 then
        -- POST request every 5th call
        return wrk.format("POST", path, nil, '{"action":"test"}')
    else
        return wrk.format("GET", path)
    end
end
