import nextConfig from 'eslint-config-next';

/** @type {import('eslint').Linter.Config[]} */
const eslintConfig = [
  {
    ignores: ['.next/**', 'out/**', 'node_modules/**', 'next-env.d.ts'],
  },
  ...nextConfig.map((cfg) => {
    if (cfg.plugins && cfg.plugins['@typescript-eslint']) {
      return {
        ...cfg,
        rules: {
          ...cfg.rules,
          '@typescript-eslint/no-explicit-any': 'off',
          '@typescript-eslint/no-unused-vars': [
            'warn',
            { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
          ],
          'react/no-unescaped-entities': 'off',
          'react-hooks/set-state-in-effect': 'off',
        },
      };
    }
    return cfg;
  }),
];

export default eslintConfig;
