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

  try {
    const contents = await getChallenge(hostname, token);

    if (contents) {
      return new Response(contents, { headers: { "content-type": "text/plain" } });
    }

    return new Response("", { status: 404 });
  } catch (err) {
    return new Response(`acme challenge error: ${err.message}`, { status: 500 });
  }
}

async function getChallenge(hostname: string, token: string): Promise<string> {
  const fbb = flatbuffers.createBuilder();
  const fbHostname = fbb.createString(hostname);
  const fbToken = fbb.createString(token);

  fbs.AcmeGetChallenge.startAcmeGetChallenge(fbb);
  fbs.AcmeGetChallenge.addHostname(fbb, fbHostname);
  fbs.AcmeGetChallenge.addToken(fbb, fbToken);

  const resp = await sendAsync(fbb, fbs.Any.AcmeGetChallenge, fbs.AcmeGetChallenge.endAcmeGetChallenge(fbb));
  const msg = new fbs.AcmeGetChallengeReady();
  resp.msg(msg);

  return msg.contents();
}
