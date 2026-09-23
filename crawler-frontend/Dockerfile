FROM node:22-alpine AS builder
WORKDIR /app
COPY crawler-frontend/package.json crawler-frontend/package-lock.json ./
RUN npm ci
COPY crawler-frontend ./
RUN npm run build

FROM nginx:1.30.5-alpine3.24
RUN sed -i '/^user /d; s#/run/nginx.pid#/tmp/nginx.pid#' /etc/nginx/nginx.conf
COPY crawler-frontend/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
EXPOSE 8080
USER 101:101
CMD ["nginx", "-g", "daemon off;"]
