FROM node:24.19.0-bookworm-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03

WORKDIR /app
COPY --chown=node:node package.json package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund
# Source checkouts on the long-term host are deliberately owner-private. Make runtime ownership
# explicit instead of relying on the checkout's incidental read bits; CI clones are usually 0644
# and would otherwise fail to reproduce this host boundary.
COPY --chown=node:node . .
RUN npm run build

ENV NODE_ENV=production
ENV PORT=3000
USER node
EXPOSE 3000
CMD ["npm", "start"]
