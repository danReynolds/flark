import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://danreynolds.github.io',
  base: '/flark',
  devToolbar: { enabled: false },
  integrations: [
    starlight({
      title: 'Flark',
      description: 'Live Markdown editing for Flutter and Fleury.',
      customCss: ['./src/styles/docs.css'],
      components: {
        Header: './src/components/SiteHeader.astro',
        Hero: './src/components/HomeHero.astro',
      },
      sidebar: [
        { label: 'Home', slug: '' },
        { label: 'Guides', items: [
          { label: 'Using Flark', slug: 'guides/using-flark' },
          { label: 'DX review decisions', slug: 'guides/dx-review' },
        ] },
      ],
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 2 },
    }),
  ],
});
