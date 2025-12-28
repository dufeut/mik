// hello.c - WASI HTTP handler in C
#include "gen/proxy.h"
#include <string.h>

// Helper: borrow from owned types
static inline wasi_http_types_borrow_outgoing_response_t borrow_response(wasi_http_types_own_outgoing_response_t o) {
    return (wasi_http_types_borrow_outgoing_response_t){ o.__handle };
}
static inline wasi_http_types_borrow_outgoing_body_t borrow_body(wasi_http_types_own_outgoing_body_t o) {
    return (wasi_http_types_borrow_outgoing_body_t){ o.__handle };
}
static inline wasi_io_streams_borrow_output_stream_t borrow_stream(wasi_io_streams_own_output_stream_t o) {
    return (wasi_io_streams_borrow_output_stream_t){ o.__handle };
}

// The response body
static const char* RESPONSE_BODY = "{\"message\":\"Hello from C!\",\"lang\":\"c\"}";

// WASI HTTP incoming-handler export
void exports_wasi_http_incoming_handler_handle(
    exports_wasi_http_incoming_handler_own_incoming_request_t request,
    exports_wasi_http_incoming_handler_own_response_outparam_t response_out
) {
    // Drop the incoming request (we don't use it)
    wasi_http_types_incoming_request_drop_own(request);

    // Create empty headers
    wasi_http_types_own_fields_t headers = wasi_http_types_constructor_fields();

    // Create outgoing response with headers
    wasi_http_types_own_outgoing_response_t response = wasi_http_types_constructor_outgoing_response(headers);

    // Set status code to 200
    wasi_http_types_method_outgoing_response_set_status_code(borrow_response(response), 200);

    // Get body handle
    wasi_http_types_own_outgoing_body_t body;
    wasi_http_types_method_outgoing_response_body(borrow_response(response), &body);

    // Get output stream
    wasi_io_streams_own_output_stream_t stream;
    wasi_http_types_method_outgoing_body_write(borrow_body(body), &stream);

    // Set the response (must be done before writing body per WASI HTTP spec)
    wasi_http_types_result_own_outgoing_response_error_code_t result;
    result.is_err = false;
    result.val.ok = response;
    wasi_http_types_static_response_outparam_set(response_out, &result);

    // Write response body
    size_t len = strlen(RESPONSE_BODY);
    proxy_list_u8_t body_bytes = { (uint8_t*)RESPONSE_BODY, len };
    wasi_io_streams_stream_error_t err;
    wasi_io_streams_method_output_stream_blocking_write_and_flush(borrow_stream(stream), &body_bytes, &err);

    // Drop the stream
    wasi_io_streams_output_stream_drop_own(stream);

    // Finish the body (no trailers)
    wasi_http_types_error_code_t body_err;
    wasi_http_types_static_outgoing_body_finish(body, NULL, &body_err);
}
