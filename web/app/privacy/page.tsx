import type { Metadata } from "next";
import { PublicPolicyPage, type PublicPolicy } from "../components/PublicPolicyPage";

export const metadata: Metadata = { title: "Privacy notice" };

const policy: PublicPolicy = {
  title: "Privacy notice",
  status: "Public browsing notice · effective August 9, 2026",
  summary: "The public observatory is built to work without advertising, visitor analytics, or cross-site tracking.",
  sections: [
    {
      heading: "Online tracking disclosure",
      paragraphs: [
        "Public pages do not load advertising pixels, analytics tags, session-replay tools, chat widgets, social-media trackers, browser-fingerprinting services, or third-party fonts. The service does not sell personal information or share it for cross-context behavioral advertising. Following a life before accounts exist uses local browser storage and is not transmitted to us.",
        "Global Privacy Control and Do Not Track signals do not change public-page behavior because the site does not perform the sale, sharing, advertising, or cross-site tracking those signals address. If that practice ever changes, the change requires a new public notice and an effective opt-out mechanism before deployment.",
      ],
    },
    {
      heading: "What the service records",
      paragraphs: [
        "Anonymous visitors can read the public world without an account. A request necessarily exposes routing information such as an IP address, requested path, timestamp, browser headers, and security signals to Cloudflare and the origin so they can deliver the page, prevent abuse, and diagnose failures. We do not add a persistent visitor identifier to public browsing.",
        "After sign-in is enabled, the service stores the identity provider, its stable opaque account identifier, and timestamps. It does not retain the email address offered by an identity provider. Session, CSRF, OAuth state, nonce, PKCE, and browser-binding secrets are stored only as hashes.",
        "For supporter purchases, the service records the submitted alias and eligibility choices, an internal account identifier, Stripe transaction references, amount and currency evidence, moderation state, and any eventual match. Stripe—not this service—handles card and wallet credentials.",
      ],
    },
    {
      heading: "Operations and processors",
      paragraphs: [
        "Cloudflare processes routing and security data as the site's network-delivery and abuse-prevention provider. Application traces omit authorization codes, cookies, OAuth query strings, and payment credentials. Public pages send no visitor data to simulation cognition providers, and observer data is never used to steer the simulation.",
        "Processors may include Apple or Google only after a visitor chooses sign-in, the newsletter provider only after a visitor follows a hosted signup link, and Stripe only after a visitor chooses payment. The newsletter provider receives the address and frequency choice directly; this service does not proxy the form or receive its subscriber list. Third-party scientific-source links receive a normal web request only if a visitor follows the link. Remote simulation cognition is operationally separate from observer traffic.",
      ],
    },
    {
      heading: "Retention and requests",
      paragraphs: [
        "Browser sessions, once enabled, expire after 30 days and can be revoked. Newsletter signup, frequency preferences, delivery, and unsubscription are handled on the chosen newsletter provider's hosted pages; A Tiny Civilization does not receive or retain its subscriber list. Security and transaction evidence is retained only as reasonably necessary for service operation, fraud prevention, accounting, disputes, and legal obligations. Public aliases may remain in an archived record, although an abusive or privacy-invasive alias can be hidden without rewriting world history.",
        "Requests for access, correction, revocation, or deletion where applicable may be sent to privacy@atinycivilization.com. Account and payment activation remains blocked until that mailbox, the legal operator identity, and the final processor inventory are verified.",
      ],
    },
  ],
};

export default function PrivacyPage() { return <PublicPolicyPage policy={policy} />; }
