# hello-python - WASI HTTP component in Python using componentize-py
from wit_world import exports
from wit_world.imports.types import (
    IncomingRequest, ResponseOutparam, OutgoingResponse, Fields, OutgoingBody
)

class IncomingHandler(exports.IncomingHandler):
    def handle(self, request: IncomingRequest, response_out: ResponseOutparam):
        # Create empty fields (headers)
        fields = Fields()

        # Create response
        response = OutgoingResponse(fields)
        response.set_status_code(200)
        body = response.body()

        # Set response BEFORE writing body (WASI HTTP requirement)
        ResponseOutparam.set(response_out, response)

        # Write body
        body.write().blocking_write_and_flush(
            b'{"message":"Hello from Python!","lang":"python"}'
        )
        OutgoingBody.finish(body, None)
