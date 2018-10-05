FROM node:10 as v8env

ADD fly fly

WORKDIR ./fly/packages/v8env
RUN yarn install
RUN rollup -c