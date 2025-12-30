// src/component.ts
import {
  ResponseOutparam,
  OutgoingBody,
  OutgoingResponse,
  Fields
} from "wasi:http/types@0.2.0";
var incomingHandler = {
  handle(_incomingRequest, responseOutparam) {
    const response = new OutgoingResponse(new Fields());
    response.setStatusCode(200);
    const body = response.body();
    const stream = body.write();
    stream.blockingWriteAndFlush(
      new Uint8Array(
        new TextEncoder().encode(
          JSON.stringify({
            message: "Hello from TypeScript!",
            service: "hello-typescript",
            lang: "typescript"
          })
        )
      )
    );
    stream[Symbol.dispose]();
    OutgoingBody.finish(body, void 0);
    ResponseOutparam.set(responseOutparam, { tag: "ok", val: response });
  }
};
export {
  incomingHandler
};
