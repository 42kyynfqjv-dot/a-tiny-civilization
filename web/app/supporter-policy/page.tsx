import type { Metadata } from "next";
import { PublicPolicyPage, type PublicPolicy } from "../components/PublicPolicyPage";

export const metadata: Metadata = { title: "Supporter naming policy" };

const policy: PublicPolicy = {
  title: "Supporter naming policy",
  status: "Pre-launch policy · effective when supporter purchases are enabled",
  summary: "A reservation can name a matching future birth. It cannot cause one, protect one, or change the world.",
  sections: [
    {
      heading: "What a reservation means",
      paragraphs: [
        "A purchase reserves one observer-visible alias for the next naturally occurring eligible birth matching the selected world, person or animal species, and birth category. It never creates, schedules, delays, protects, strengthens, or controls a life. There is no promised fulfilment date, and extinction may make fulfilment impossible.",
        "Support is not ownership of an organism, world, story, trademark, or scientific result. A named life may be injured or die, and the world may become extinct.",
      ],
    },
    {
      heading: "Moderation",
      paragraphs: [
        "Every alias passes automatic screening and human review. Profanity, slurs, harassment, sexual content, threats, advertising, personal data, impersonation, and attempts to evade these rules are rejected. Automatic acceptance is never final approval.",
        "Aliases cannot be sold or transferred across accounts, worlds, roles, species, or birth categories. An abusive or legally risky alias may later be hidden while the underlying organism and canonical history remain unchanged.",
      ],
    },
    {
      heading: "Cancellation and refunds",
      paragraphs: [
        "A full refund is provided for a rejected paid alias without an accepted replacement, a verified duplicate charge, or extinction before matching. An unmatched supporter may also cancel through their account. Paid cancellations use the same idempotent full-refund path; unpaid cancellations never contact Stripe.",
        "After an alias is matched and published, a purchase is final except where law requires otherwise or the service materially failed to provide this policy. Questions go to support@atinycivilization.com; that monitored mailbox and moderation response process must exist before payments are enabled.",
      ],
    },
  ],
};

export default function SupporterPolicyPage() { return <PublicPolicyPage policy={policy} />; }
