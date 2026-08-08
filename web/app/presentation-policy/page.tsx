import type { Metadata } from "next";
import { PublicPolicyPage, type PublicPolicy } from "../components/PublicPolicyPage";

export const metadata: Metadata = { title: "World presentation policy" };

const policy: PublicPolicy = {
  title: "World presentation policy",
  status: "Accepted project policy",
  summary: "The public record stays factual, restrained, and suitable for a broad audience—even when the world is difficult.",
  sections: [
    {
      heading: "Restrained public record",
      paragraphs: [
        "The simulation can contain birth, injury, predation, conflict, death, and extinction as restrained mechanical facts. The observatory never presents sexual activity, reproductive mechanisms, graphic violence, gore, or sensationalized suffering.",
        "Public birth records omit reproductive partners, private development, sex mechanism, and parentage detail. Death records use neutral, non-graphic language and may group or withhold detail when presentation would be exploitative or unsafe.",
      ],
    },
    {
      heading: "No invented drama",
      paragraphs: [
        "Observer summaries are deterministic finding aids grounded in cited events. They do not invent dialogue, motives, feelings, or dramatic narration. Supporter aliases never turn a life into a protected character or give an observer control.",
        "The inhabitants are bounded computational agents. The project does not claim consciousness, sentience, human-equivalent experience, or moral personhood, and it does not use that disclaimer to justify gratuitous presentation.",
      ],
    },
    {
      heading: "Audit without spectacle",
      paragraphs: [
        "Public verification exposes payload-free commitments. Detailed technical material must pass the same presentation boundary and cannot turn private mechanisms into explicit public spectacle.",
      ],
    },
  ],
};

export default function PresentationPolicyPage() { return <PublicPolicyPage policy={policy} />; }
