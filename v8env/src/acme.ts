import { sendAsync } from './bridge';
import * as fbs from "./msg_generated";
import * as flatbuffers from "./flatbuffers"
import { FlyRequest } from './request';
import { FlyResponse } from './response';

const ACME_CHALLENGE_PATH_PREFIX = "/.well-known/acme-challenge/";

export function isAcmeChallengeRequest(req: FlyRequest): boolean {
  const url = new URL(req.url);
  if (!url.pathname.startsWith(ACME_CHALLENGE_PATH_PREFIX)) {
    return false;
  }
  if (!req.headers.has("host")) {
    return false;
  }
  return true;
}

export async function handleAcmeChallenge(req: FlyRequest): Promise<FlyResponse> {
  const url = new URL(req.url);
  const hostname = req.headers.get("host");

  const token = url.pathname.substring(ACME_CHALLENGE_PATH_PREFIX.length);

  console.log("handleAcmeChallenge", { hostname, token });

  const { valid, contents } = await validateChallenge(hostname, token).catch(err => {
    console.error("Error validating acme challenge");
    return { valid: false, contents: "" };
  });

  console.log("handleAcmeChallenge.response", { valid, contents });

  return new Response(valid ? contents : "invalid", {
    status: valid ? 200 : 404,
    headers: {
      "content-type": "text/plain"
    }
  });
}

async function validateChallenge(hostname: string, token: string): Promise<{valid: boolean, contents: string}> {
  const fbb = flatbuffers.createBuilder();
  const fbHostname = fbb.createString(hostname);
  const fbToken = fbb.createString(token);

  fbs.AcmeValidateChallenge.startAcmeValidateChallenge(fbb);
  fbs.AcmeValidateChallenge.addHostname(fbb, fbHostname);
  fbs.AcmeValidateChallenge.addToken(fbb, fbToken);

  const resp = await sendAsync(fbb, fbs.Any.AcmeValidateChallenge, fbs.AcmeValidateChallenge.endAcmeValidateChallenge(fbb));
  const msg = new fbs.AcmeValidateChallengeReady();
  resp.msg(msg);

  return {
    valid: msg.valid(),
    contents: msg.contents()
  };
}
