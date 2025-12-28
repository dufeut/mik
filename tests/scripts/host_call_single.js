// Script that calls a single WASM module
export default function(input) {
    var result = host.call("echo", {
        method: "POST",
        path: "/",
        body: input
    });
    return result;
}
