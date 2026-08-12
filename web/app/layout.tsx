import type { Metadata } from "next";
import { IBM_Plex_Mono, Inter, Libre_Baskerville } from "next/font/google";
import "./globals.css";

const bookSerif = Libre_Baskerville({
  variable: "--font-book-serif",
  subsets: ["latin"],
  weight: ["400", "700"],
});

const bookSans = Inter({
  variable: "--font-book-sans",
  subsets: ["latin"],
});

const bookMono = IBM_Plex_Mono({
  variable: "--font-book-mono",
  subsets: ["latin"],
  weight: ["400", "500", "600"],
});

export const metadata: Metadata = {
  title: { default: "Building EvoCC", template: "%s · Building EvoCC" },
  description: "A visual, executable notebook about building a proof-carrying GNU C11 compiler.",
  icons: { icon: "/favicon.svg", shortcut: "/favicon.svg" },
  openGraph: {
    title: "Building EvoCC",
    description: "A visual, executable notebook about building a proof-carrying GNU C11 compiler.",
    type: "article",
    images: [{ url: "/evocc-compiler-map.png", width: 1822, height: 863 }],
  },
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body className={`${bookSerif.variable} ${bookSans.variable} ${bookMono.variable}`}>
        {children}
      </body>
    </html>
  );
}
