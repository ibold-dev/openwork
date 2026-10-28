import type { NextConfig } from 'next';

const apiBaseUrl = process.env.OPENWORK_API_URL ?? 'http://127.0.0.1:8080';

const nextConfig: NextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  // The Docker image runs the server bundle from `.next/standalone`.
  output: 'standalone',
  transpilePackages: ['@openwork/sdk'],
  async rewrites() {
    // Same-origin proxy so the API's HttpOnly session cookie is first-party.
    return [
      {
        source: '/api/proxy/:path*',
        destination: `${apiBaseUrl}/:path*`,
      },
    ];
  },
  async headers() {
    return [
      {
        source: '/:path*',
        headers: [
          { key: 'X-Content-Type-Options', value: 'nosniff' },
          { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
          { key: 'X-Frame-Options', value: 'DENY' },
          {
            key: 'Permissions-Policy',
            value: 'camera=(), microphone=(), geolocation=()',
          },
        ],
      },
    ];
  },
};

export default nextConfig;
