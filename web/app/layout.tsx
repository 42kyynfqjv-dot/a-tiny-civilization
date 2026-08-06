import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { headers } from "next/headers";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export async function generateMetadata(): Promise<Metadata> {
  const requestHeaders = await headers();
  const forwardedHost = requestHeaders.get("x-forwarded-host");
  const host = (forwardedHost ?? requestHeaders.get("host") ?? "localhost:3000").split(",")[0].trim();
  const forwardedProtocol = requestHeaders.get("x-forwarded-proto")?.split(",")[0].trim();
  const protocol = forwardedProtocol ?? (host.startsWith("localhost") ? "http" : "https");
  const metadataBase = safeOrigin(protocol, host);

  return {
    metadataBase,
    title: {
      default: "A Tiny Civilization Observatory",
      template: "%s · A Tiny Civilization",
    },
    description:
      "Watch an unscripted civilization live, trace every claim to evidence, and explore the worlds it leaves behind.",
    openGraph: {
      type: "website",
      title: "A Tiny Civilization",
      description: "History without a script.",
      images: [{ url: "/og.png", width: 1731, height: 909, alt: "A Tiny Civilization river-basin observatory" }],
    },
    twitter: {
      card: "summary_large_image",
      title: "A Tiny Civilization",
      description: "History without a script.",
      images: ["/og.png"],
    },
  };
}

function safeOrigin(protocol: string, host: string): URL {
  try {
    return new URL(`${protocol === "http" ? "http" : "https"}://${host}`);
  } catch {
    return new URL("http://localhost:3000");
  }
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        {children}
      </body>
    </html>
  );
}
