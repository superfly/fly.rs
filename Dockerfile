FROM node:10 as v8env

ADD v8env v8env

WORKDIR ./v8env
RUN yarn install
RUN ./node_modules/.bin/rollup -c