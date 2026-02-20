import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://chattor.netlify.app',
  integrations: [
    starlight({
      title: 'chattor',
      logo: {
        src: './src/assets/chattor-logo.png',
        replacesTitle: true,
      },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/jmsingleton/chattor' },
      ],
      editLink: {
        baseUrl: 'https://github.com/jmsingleton/chattor/edit/main/website/',
      },
      customCss: [
        './src/styles/custom.css',
      ],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            'getting-started/installation',
            'getting-started/quickstart',
          ],
        },
        {
          label: 'Guides',
          items: [
            'guides/theming',
            'guides/friend-codes',
          ],
        },
        {
          label: 'Architecture',
          items: [
            'architecture/overview',
            'architecture/signal-protocol',
            'architecture/tor-integration',
          ],
        },
        {
          label: 'Reference',
          items: [
            'faq',
            'contributing',
          ],
        },
      ],
    }),
  ],
});
