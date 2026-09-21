import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  devToolbar: { enabled: false },
  integrations: [
    starlight({
      title: 'Flark',
      description: 'Live Markdown editing for Flutter and Fleury.',
      customCss: ['./src/styles/docs.css'],
      sidebar: [
        { label: 'Introduction', slug: '' },
        { label: 'Guides', items: [
          { label: 'Using Flark', slug: 'guides/using-flark' },
          { label: 'DX review decisions', slug: 'guides/dx-review' },
        ] },
      ],
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 2 },
    }),
  ],
});
