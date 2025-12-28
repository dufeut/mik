// Script that tests sandbox restrictions
export default function(input) {
    return {
        has_fetch: typeof fetch !== "undefined",
        has_require: typeof require !== "undefined",
        has_process: typeof process !== "undefined",
        has_fs: typeof fs !== "undefined",
        has_eval: typeof eval !== "undefined",
        has_host: typeof host !== "undefined",
        has_host_call: typeof host.call === "function"
    };
}
