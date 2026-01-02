-- Basic load test script for mik
wrk.method = "GET"
wrk.headers["Content-Type"] = "application/json"

request = function()
    return wrk.format("GET", "/run/hello/")
end

response = function(status, headers, body)
    if status ~= 200 then
        print("Error: " .. status)
    end
end
