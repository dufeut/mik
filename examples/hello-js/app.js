// hello-js - WASI HTTP component in JavaScript using jco
import {
  ResponseOutparam, OutgoingBody, OutgoingResponse, Fields,
} from 'wasi:http/types@0.2.0';

export const incomingHandler = {
  handle(incomingRequest, responseOutparam) {
    const path = incomingRequest.pathWithQuery() || '/';

    const response = new OutgoingResponse(new Fields());
    response.setStatusCode(200);

    let body = response.body();
    let stream = body.write();
    stream.blockingWriteAndFlush(
      new Uint8Array(new TextEncoder().encode(
        JSON.stringify({ message: "Hello from JavaScript!", lang: "javascript", path })
      ))
    );
    stream[Symbol.dispose]();
    OutgoingBody.finish(body, undefined);
    ResponseOutparam.set(responseOutparam, { tag: 'ok', val: response });
  }
};
