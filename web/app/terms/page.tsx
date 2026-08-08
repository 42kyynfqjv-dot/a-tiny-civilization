import type { Metadata } from "next";
import { PublicPolicyPage, type PublicPolicy } from "../components/PublicPolicyPage";

export const metadata: Metadata = { title: "Terms of use" };

const policy: PublicPolicy = {
  title: "Terms of use",
  status: "Pre-launch terms · effective when the public service is enabled",
  summary: "A Tiny Civilization is an open experiment, not a promise that civilization—or even survival—will happen.",
  sections: [
    {
      heading: "The experiment",
      paragraphs: [
        "The service makes no promise that reproduction, discovery, survival, availability, or any particular event will occur. Public projections may be delayed, corrected, or scientifically superseded, but canonical history is not edited to make it more entertaining.",
        "Visitors may browse the public record and use project software under Apache-2.0. That software license does not grant rights in third-party datasets, provider marks, supporter aliases, submissions, or project branding where separate rights apply.",
      ],
    },
    {
      heading: "Responsible access",
      paragraphs: [
        "Do not attack the service, evade access or moderation controls, harm availability through scraping, submit unlawful or abusive content, impersonate another person, expose personal data, or misrepresent provisional outputs as scientific findings.",
        "The service may suspend accounts, reject observer content, rate-limit traffic, or pause public access for integrity, safety, maintenance, legal compliance, or resource exhaustion. A capacity pause occurs only at a committed simulation boundary and cannot authorize hidden changes to history.",
      ],
    },
    {
      heading: "Accounts and purchases",
      paragraphs: [
        "Account holders are responsible for their identity-provider account and browser sessions. Supporter purchases follow the separate naming policy and are not investments, charitable tax receipts, ownership interests, or guarantees of fulfilment or influence.",
        "These terms require final legal review plus the operator identity, governing law, contact details, and jurisdiction-appropriate liability language. Until that work is complete, account and payment routes remain configuration-disabled. Non-waivable consumer rights always remain intact.",
      ],
    },
  ],
};

export default function TermsPage() { return <PublicPolicyPage policy={policy} />; }
