#! /usr/bin/env bash
set +xev

graphql-client introspect-schema https://bff.kivra.com/graphql > schemas/kivra.json

#\
  #  --authorization "Bearer $KIVRA_API_KEY" \
  #  --header "x-actor-type: user" \
  #  --header "x-actor-key: $KIVRA_ACTOR_KEY"  
