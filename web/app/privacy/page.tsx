import type { Metadata } from "next";
import { PublicPolicyPage, type PublicPolicy } from "../components/PublicPolicyPage";

export const metadata: Metadata = { title: "Privacy notice" };

const policy: PublicPolicy = {
  title: "Privacy notice",
  status: "Pre-launch notice · effective when public accounts are enabled",
  summary: "The world is public. Observer accounts and payments are separate, minimal, and never allowed to steer it.",
  sections: [
    {
      heading: "What the service records",
      paragraphs: [
        "Anonymous visitors can read the public world without an account. After sign-in, the service stores the identity provider, its stable account identifier, an optional verified email address, and timestamps. Session, CSRF, OAuth state, nonce, PKCE, and browser-binding secrets are stored only as hashes.",
        "For supporter purchases, the service records the submitted alias and eligibility choices, an internal account identifier, Stripe transaction references, amount and currency evidence, moderation state, and any eventual match. Stripe—not this service—handles card and wallet credentials.",
      ],
    },
    {
      heading: "Operations and processors",
      paragraphs: [
        "Infrastructure necessarily processes IP addresses, request metadata, security logs, and short-lived diagnostics. Application traces omit authorization codes, cookies, OAuth query strings, and payment credentials. Observer data is not sold and is never used to steer the simulation.",
        "Processors may include Apple or Google for sign-in, Stripe for payment, Cloudflare for network delivery, and the hosting and database operators. Remote cognition remains disabled unless its private simulated-state export is separately approved and disclosed here before activation.",
      ],
    },
    {
      heading: "Retention and requests",
      paragraphs: [
        "Browser sessions expire after 30 days and can be revoked. Security and transaction evidence is retained as reasonably necessary for fraud, accounting, disputes, and legal obligations. Public aliases may remain in an archived record, although an abusive or privacy-invasive alias can be hidden without rewriting world history.",
        "Requests for access, correction, revocation, or deletion where applicable may be sent to privacy@atinycivilization.com. That monitored mailbox, the legal operator identity, jurisdiction-specific rights, and the final processor list must be completed before accounts are enabled.",
      ],
    },
  ],
};

export default function PrivacyPage() { return <PublicPolicyPage policy={policy} />; }
