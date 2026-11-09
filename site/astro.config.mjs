// SPDX-License-Identifier: Apache-2.0
// Docs site and in-browser playground for storage-schema (SSC-027).
// Kit theme: design/astro (copied into src/, ADR-0002: projects stay
// independent). Static output; set `site` (and `base` if served under a
// path) when the hosting URL is known (DEPLOY.md).
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const PROJECT = 'storage-schema';

export default defineConfig({
  integrations: [
    starlight({
      title: 'storage-schema',
      favicon: '/favicon.svg',
      description:
        'An informational storage schema for Soroban contracts (contractstoragev0): which keys a contract writes, with what durability and type, so off-chain tools can name and type raw ledger entries.',
      head: [{ tag: 'script', content: `document.documentElement.dataset.project=${JSON.stringify(PROJECT)}` }],
      customCss: [
        '@fontsource-variable/geist',
        '@fontsource-variable/geist-mono',
        './src/styles/kit/tokens.css',
        './src/styles/kit/starlight-theme.css',
        './src/styles/site.css',
      ],
      components: { SiteTitle: './src/components/kit/SiteTitle.astro' },
      expressiveCode: {
        themes: ['github-dark-default', 'github-light-default'],
        styleOverrides: {
          borderRadius: '6px',
          borderColor: 'var(--sik-line)',
          codeFontFamily: 'var(--sik-font-mono)',
          uiFontFamily: 'var(--sik-font-sans)',
        },
      },
      sidebar: [
        {
          label: 'Start',
          items: [
            { label: 'Overview', link: '/' },
            { label: 'Playground', link: '/playground/' },
            { label: 'Size cost', link: '/size/' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'Quickstart (CLI)', link: '/guides/quickstart/' },
            { label: 'Annotate a contract', link: '/guides/annotate/' },
            { label: 'Keys written by dependencies', link: '/guides/dependencies/' },
            { label: 'Sidecar files', link: '/guides/sidecar/' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Section format', link: '/reference/format/' },
            { label: 'Matching and statuses', link: '/reference/matching/' },
            { label: 'Browser engine', link: '/reference/engine/' },
            { label: 'SEP draft', link: '/sep-draft/' },
          ],
        },
        { label: 'Project', items: [{ label: 'Status and limits', link: '/status/' }] },
      ],
    }),
  ],
});
