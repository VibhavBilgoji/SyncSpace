// A fake GitHub for the end-to-end tests (`e2e/tests/github.spec.ts`): the OAuth screens, the token
// exchange, the profile and email endpoints, and the public repositories of a user. Tests choose
// what it answers with `POST /__scenario`. It only listens on the loopback interface and never
// talks to the real GitHub.

import { createServer } from "node:http";

const PORT = Number(process.env.MOCK_GITHUB_PORT ?? 4010);
const TOKEN = "gho_e2e_fake_token";

let scenario = emptyScenario();

function emptyScenario() {
  return { id: 1, login: "octocat", name: "The Octocat", emails: [], repos: [], deny: false };
}

function send(res, status, body, headers = {}) {
  res.writeHead(status, {
    "Content-Type": "application/json",
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Headers": "*",
    ...headers,
  });
  res.end(body === undefined ? "" : JSON.stringify(body));
}

async function readJson(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const text = Buffer.concat(chunks).toString("utf8");
  return text ? JSON.parse(text) : {};
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", `http://127.0.0.1:${PORT}`);
  try {
    if (req.method === "OPTIONS") return send(res, 204);
    if (url.pathname === "/__health") return send(res, 200, { ok: true });

    if (url.pathname === "/__scenario" && req.method === "POST") {
      scenario = { ...emptyScenario(), ...(await readJson(req)) };
      return send(res, 200, { ok: true });
    }

    // What the person sees on GitHub: they approve at once (or decline), and GitHub sends them
    // back to the callback with a code and the same state.
    if (url.pathname === "/login/oauth/authorize") {
      const back = new URL(url.searchParams.get("redirect_uri") ?? "");
      const state = url.searchParams.get("state") ?? "";
      if (scenario.deny) {
        back.searchParams.set("error", "access_denied");
      } else {
        back.searchParams.set("code", "e2e-code");
      }
      back.searchParams.set("state", state);
      res.writeHead(302, { Location: back.toString() });
      return res.end();
    }

    if (url.pathname === "/login/oauth/access_token" && req.method === "POST") {
      const body = await readJson(req);
      if (body.client_id !== "e2e-client" || body.client_secret !== "e2e-secret") {
        return send(res, 200, { error: "incorrect_client_credentials" });
      }
      return send(res, 200, { access_token: TOKEN, token_type: "bearer" });
    }

    const authorised = req.headers.authorization === `Bearer ${TOKEN}`;
    if (url.pathname === "/user") {
      if (!authorised) return send(res, 401, { message: "Bad credentials" });
      return send(res, 200, { id: scenario.id, login: scenario.login, name: scenario.name });
    }
    if (url.pathname === "/user/emails") {
      if (!authorised) return send(res, 401, { message: "Bad credentials" });
      return send(res, 200, scenario.emails);
    }

    const repos = /^\/users\/([^/]+)\/repos$/.exec(url.pathname);
    if (repos) {
      if (repos[1] !== scenario.login) return send(res, 404, { message: "Not Found" });
      const perPage = Number(url.searchParams.get("per_page") ?? 30);
      const page = Number(url.searchParams.get("page") ?? 1);
      return send(res, 200, scenario.repos.slice((page - 1) * perPage, page * perPage));
    }

    return send(res, 404, { message: "Not Found" });
  } catch (error) {
    return send(res, 500, { message: String(error) });
  }
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`fake GitHub on http://127.0.0.1:${PORT}`);
});
